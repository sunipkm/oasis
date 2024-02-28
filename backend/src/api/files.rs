#![allow(unused_imports)]
use crate::entity::copy_move_task::{CopyMoveFileRequest, CopyMoveTask};
use crate::entity::error::Error;
use crate::entity::file::{File, FileType};
use crate::entity::hidden::Hidden;
use crate::entity::request::{
    CreateDirRequest, GenerateLinkRequest, RenameFileRequest, SetFileVisibilityRequest,
};
use crate::entity::response::FileResponse;
use crate::service::app_state::AppState;
use crate::service::auth::{AuthAdmin, AuthUser};
use crate::service::range::{Range, RangedFile};
use crate::service::track;
use crate::util::constants::ZIP_BUFFER_SIZE;
use crate::util::{self, file_system};
use anyhow::Result as AnyResult;
use async_zip::{write::ZipFileWriter, Compression, ZipEntryBuilder};
use rocket::fs::NamedFile;
use rocket::response::stream::ByteStream;
use rocket::serde::json::Json;
use rocket::tokio::fs;
use rocket::tokio::io::{self, AsyncReadExt, AsyncWrite};
use rocket::{Route, State};
use sqlx::Acquire;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn route() -> Vec<Route> {
    routes![
        dir_content,
        create_dir,
        update_file_name,
        delete_file,
        file_content,
        video_track,
        generate_share_link,
        get_share_link,
        search_files,
        update_file_visibility,
        copy_move_file,
        get_copy_move_status,
        download_dir
    ]
}

#[get("/dir?<path>")]
async fn dir_content(
    path: Option<&str>,
    user: AuthUser,
    state: &State<AppState>,
) -> Result<Json<Vec<File>>, Error> {
    let storage = state.get_site()?.storage.clone();
    let target_path = match path {
        Some(dir) => PathBuf::from(&storage).join(util::parse_encoded_url(dir)?),
        None => PathBuf::from(&storage),
    };

    if !target_path.exists() || !target_path.is_dir() {
        eprintln!("Invalid dir path: {:?}", &target_path);
        return Err(Error::BadRequest);
    }

    let mut conn = state.get_pool_conn().await?;
    let hiddens = Hidden::find_all(&mut conn).await?;

    if get_least_permission(&target_path, &storage, &hiddens) > user.permission {
        return Err(Error::Unauthorized);
    }

    let mut dir_iterator = fs::read_dir(target_path).await?;
    let mut content: Vec<File> = Vec::new();
    while let Some(entry) = dir_iterator.next_entry().await? {
        let path = entry.path();
        let least_permission = get_least_permission(&path, &storage, &hiddens);
        if least_permission <= user.permission {
            content.push(File::from_path(&path, false, &storage, least_permission)?);
        }
    }

    Ok(Json(content))
}

#[get("/download/dir?<path>")]
async fn download_dir(
    path: &str,
    _user: AuthAdmin,
    state: &State<AppState>,
) -> ByteStream![Vec<u8>] {
    let (mut writer, mut reader) = tokio::io::duplex(ZIP_BUFFER_SIZE);
    let target_path = get_target_path(state, path).unwrap();
    rocket::tokio::spawn(async move {
        if let Err(e) = zip_dir(&mut writer, &target_path).await {
            println!("Error ziping dir: {}", e);
        }
    });

    ByteStream! {
        loop {
            let mut buf = vec![0; ZIP_BUFFER_SIZE];
            let r = reader.read(&mut buf).await.unwrap();
            if r == 0 {
                break;
            }
            buf.truncate(r);
            yield buf;
        }
    }
}

#[post("/dir", data = "<req_body>")]
async fn create_dir(
    state: &State<AppState>,
    req_body: Json<CreateDirRequest>,
    _user: AuthAdmin,
) -> Result<(), Error> {
    let storage = state.get_site()?.storage.clone();
    let parent = util::parse_encoded_url(&req_body.parent)?;
    let parent_path = PathBuf::from(storage).join(&parent);
    let target_path = parent_path.join(&req_body.name);

    if !parent_path.exists() || target_path.exists() {
        return Err(Error::BadRequest);
    }

    Ok(fs::create_dir(target_path).await?)
}

#[get("/file/<path>")]
async fn file_content(
    path: &str,
    user: AuthUser,
    state: &State<AppState>,
    range_header: Range,
) -> Result<FileResponse, Error> {
    let target_path = get_target_path(state, path).map_err(|e| {
        eprintln!("{}", e);
        400
    })?;

    if !target_path.is_file() {
        return Err(Error::BadRequest);
    }

    let mut conn = state.get_pool_conn().await?;
    let hiddens = Hidden::find_all(&mut conn).await?;
    let storage = state.get_site()?.storage.clone();
    if max_permission_parent(&target_path, &storage, &hiddens) > user.permission {
        return Err(Error::Unauthorized);
    }

    let is_text = FileType::get_file_type(&target_path) == FileType::Text;

    match (range_header.range, is_text) {
        (Some(range), _) => {
            let ranged_file = RangedFile::new(range, target_path).await?;
            Ok(FileResponse::Range(ranged_file))
        }
        (None, true) => Ok(FileResponse::Text(
            file_system::read_text_file(target_path).await?,
        )),
        (None, false) => Ok(FileResponse::Binary(NamedFile::open(target_path).await?)),
    }
}

#[put("/file/<path>/name", data = "<req_body>")]
async fn update_file_name(
    state: &State<AppState>,
    path: &str,
    req_body: Json<RenameFileRequest>,
    _user: AuthAdmin,
) -> Result<(), Error> {
    let current_file = get_target_path(state, path).map_err(|e| {
        eprintln!("{}", e);
        400
    })?;

    let target_file = current_file.parent().unwrap().join(&req_body.new_name);

    if target_file.exists() {
        return Err(Error::BadRequest);
    }

    fs::rename(current_file, &target_file).await?;

    let target_relative_path = get_relative_path(state, &target_file)?;
    let target_path_str = target_relative_path.to_str().unwrap();
    let mut conn = state.get_pool_conn().await?;
    let mut tx = conn.begin().await?;
    Hidden::update_all_sub_path_query(&mut tx, path, target_path_str).await?;
    tx.commit().await?;

    Ok(())
}

#[put("/file/<path>/visibility", data = "<req_body>")]
async fn update_file_visibility(
    state: &State<AppState>,
    path: &str,
    req_body: Json<SetFileVisibilityRequest>,
    _user: AuthAdmin,
) -> Result<(), Error> {
    let mut conn = state.get_pool_conn().await?;
    let mut tx = conn.begin().await?;

    if !req_body.visible {
        let hidden = Hidden::new(path, 1);
        hidden.insert_query(&mut tx).await?;
    } else {
        Hidden::delete_query(path, &mut tx).await?;
    }

    tx.commit().await?;

    Ok(())
}

#[delete("/file/<path>")]
async fn delete_file(state: &State<AppState>, path: &str, _user: AuthAdmin) -> Result<(), Error> {
    let target_path = get_target_path(state, path).map_err(|e| {
        eprintln!("{}", e);
        400
    })?;

    if target_path.is_file() {
        fs::remove_file(target_path).await?;
    } else {
        fs::remove_dir_all(target_path).await?;
    }

    let mut conn = state.get_pool_conn().await?;
    let mut tx = conn.begin().await?;
    Hidden::delete_all_sub_path_query(&mut tx, path).await?;
    tx.commit().await?;

    Ok(())
}

// Temporary solution for track file searching.
// Could remove this function in the future.
#[get("/file/track/<path>")]
async fn video_track(
    path: &str,
    _user: AuthUser,
    state: &State<AppState>,
) -> Result<String, Error> {
    let storage = state.get_site()?.storage.clone();
    let target_path = PathBuf::from(storage).join(util::parse_encoded_url(path)?);

    let track_str = match track::get_track(target_path).await {
        Ok(str) => str,
        Err(e) => {
            eprintln!("Error when getting track: {}", e);
            return Err(Error::NotFound);
        }
    };

    Ok(track_str)
}

#[post("/file/share", data = "<req_body>")]
async fn generate_share_link(
    state: &State<AppState>,
    req_body: Json<GenerateLinkRequest>,
    _user: AuthUser,
) -> Result<String, Error> {
    let secret = state.get_secret()?;
    let path_encode = urlencoding::encode(&req_body.path);
    let input = format!("expire={}&path={}", req_body.expire, path_encode);
    let hash = util::sha256(&input, &secret);

    Ok(format!("hash={}&{}", hash, input))
}

#[get("/file/share?<path>&<expire>&<hash>")]
async fn get_share_link(
    state: &State<AppState>,
    path: &str,
    expire: i64,
    hash: &str,
    range_header: Range,
) -> Result<FileResponse, Error> {
    let secret = state.get_secret()?;
    let path_encode = urlencoding::encode(path);

    let input = format!("expire={}&path={}", expire, path_encode);
    if hash != util::sha256(&input, &secret) {
        return Err(Error::BadRequest);
    }

    let target_path = get_target_path(state, path).map_err(|e| {
        eprintln!("{}", e);
        400
    })?;

    if !target_path.is_file() {
        return Err(Error::BadRequest);
    }

    match range_header.range {
        Some(range) => {
            let ranged_file = RangedFile::new(range, target_path).await?;
            Ok(FileResponse::Range(ranged_file))
        }
        None => Ok(FileResponse::Binary(NamedFile::open(target_path).await?)),
    }
}

#[get("/file/search?<keywords>")]
async fn search_files(
    state: &State<AppState>,
    keywords: &str,
    user: AuthUser,
) -> Result<Json<Vec<File>>, Error> {
    let decoded_keywords = util::parse_encoded_url(keywords)?;
    let decoded_keywords_str = decoded_keywords.as_os_str().to_str().unwrap();
    let keywords_splits: Vec<&str> = decoded_keywords_str.split("+").collect();
    let results = search_dir_all(state, &keywords_splits, user.permission).await?;

    Ok(Json(results))
}

#[post("/file/copy-move", data = "<req_body>")]
async fn copy_move_file(
    state: &State<AppState>,
    admin: AuthAdmin,
    req_body: Json<CopyMoveFileRequest>,
) -> Result<String, Error> {
    if !CopyMoveTask::allow_new_task() {
        eprintln!("Copy or move task is already running.");
        return Err(Error::BadRequest);
    }

    let source = get_target_path(state, &req_body.source)?;
    let target = get_target_path(state, &req_body.target)?;
    let task = CopyMoveTask::new(
        source,
        target,
        admin.uid,
        req_body.is_copy,
        req_body.overwrite,
    );
    task.set_static_value();
    task.run();

    Ok(task.uuid.clone())
}

#[get("/file/copy-move-status/<uuid>")]
async fn get_copy_move_status(uuid: String, admin: AuthAdmin) -> Result<Json<CopyMoveTask>, Error> {
    let task = CopyMoveTask::get_static_value();
    if task.is_none() || task.as_ref().unwrap().uuid != uuid {
        return Err(Error::NotFound);
    }

    if task.as_ref().unwrap().user_id != admin.uid {
        return Err(Error::BadRequest);
    }

    let mut task_res = task.unwrap();
    task_res.user_id = 0;

    Ok(Json(task_res))
}

fn get_target_path(state: &State<AppState>, path: &str) -> AnyResult<PathBuf> {
    let storage = state.get_site()?.storage.clone();
    let target_path = PathBuf::from(&storage).join(util::parse_encoded_url(path)?);

    if !target_path.exists() {
        return Err(anyhow::anyhow!("Invalid path: {:?}", target_path));
    }

    Ok(target_path)
}

fn get_relative_path(state: &State<AppState>, full_path: &Path) -> AnyResult<PathBuf> {
    let storage = state.get_site()?.storage.clone();
    let relative_path = full_path.strip_prefix(storage)?;

    Ok(relative_path.to_owned())
}

async fn search_dir_all(
    state: &State<AppState>,
    keywords: &[&str],
    user_permission: i8,
) -> AnyResult<Vec<File>> {
    let mut results = vec![];
    let mut conn = state.get_pool_conn().await?;
    let hiddens = Hidden::find_all(&mut conn).await?;
    let storage = state.get_site()?.storage.clone();

    for entry in WalkDir::new(&storage).follow_links(false).into_iter() {
        let entry = entry?;
        let path = entry.path();
        if contains_all_keywords(path, keywords)
            && max_permission_parent(path, &storage, &hiddens) <= user_permission
        {
            let path_buf = PathBuf::from(path);
            let file = File::from_path(&path_buf, true, &storage, 0)?;
            results.push(file);
        }
    }

    Ok(results)
}

// Check the permission setting for the exact input file path only.
fn get_least_permission(file_path: &PathBuf, storage: &str, hiddens: &Vec<Hidden>) -> i8 {
    let storage_path = PathBuf::from(storage);

    for hidden in hiddens.iter() {
        let hidden_full_path = storage_path.join(&hidden.path);

        if &hidden_full_path == file_path {
            return hidden.least_permission;
        }
    }

    0
}

// Check the max permission value of all the parents of the input file path.
fn max_permission_parent(file_path: &Path, storage: &str, hiddens: &Vec<Hidden>) -> i8 {
    let mut least_permission = 0;
    let storage_path = PathBuf::from(storage);

    for hidden in hiddens.iter() {
        let hidden_full_path = storage_path.join(&hidden.path);

        if file_path.starts_with(&hidden_full_path) && hidden.least_permission > least_permission {
            least_permission = hidden.least_permission;
        }
    }

    least_permission
}

// All keywords are lower case before sending to backend
fn contains_all_keywords(path: &Path, keywords: &[&str]) -> bool {
    let filename = path.file_name().unwrap().to_str().unwrap().to_lowercase();
    for keyword in keywords.iter() {
        let keyword_low = keyword.to_lowercase();
        if !filename.contains(&keyword_low) {
            return false;
        }

        // Keywords start with "." indicates a filter for file type.
        // Eg. ".js" means searching for js files, and json files should be excluded.
        if keyword_low.starts_with('.') {
            if path.is_dir() {
                return false;
            }

            match path.extension() {
                Some(ext) => {
                    let ext_str = ext.to_str().unwrap();
                    let ext_str_dot = format!(".{}", ext_str);
                    if ext_str_dot != keyword_low {
                        return false;
                    }
                }
                None => return false,
            };
        }
    }

    true
}

async fn zip_dir<W: AsyncWrite + Unpin>(
    writer: &mut W,
    path: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = ZipFileWriter::new(writer);
    let mut it = WalkDir::new(path).into_iter();
    while let Some(Ok(entry)) = it.next() {
        if !entry.file_type().is_file() {
            continue;
        }

        if entry.path().symlink_metadata().is_err() {
            continue;
        }

        let parent_dir = match path.parent() {
            Some(p) => p,
            None => path,
        };
        let filename = match entry.path().strip_prefix(parent_dir) {
            Ok(v) => v.to_str().unwrap(),
            Err(_) => continue,
        };
        let entry_op =
            ZipEntryBuilder::new(filename.to_owned(), Compression::Stored).unix_permissions(0o644);
        let mut file = tokio::fs::File::open(entry.path()).await?;
        let mut file_writer = writer.write_entry_stream(entry_op).await?;
        io::copy(&mut file, &mut file_writer).await?;
        file_writer.close().await?;
    }

    writer.close().await?;
    Ok(())
}
