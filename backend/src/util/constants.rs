use std::net::{IpAddr, Ipv4Addr};

pub const DEFAULT_APP_NAME: &str = "Oasis";
pub const DEFAULT_UPDATE_FREQ: &str = "monthly";
pub const DEFAULT_LANGUAGE: &str = "en";
pub const VERSION: &str = "0.2.6";
#[allow(dead_code)]
pub const FRONTEND_DIR_DEBUG: &str = "../frontend/public/";
#[allow(dead_code)]
pub const FRONTEND_DIR_RELEASE: &str = "public";
pub const ACCESS_TOKEN: &str = "oa_access";
pub const ACCESS_TOKEN_MINS: i64 = 20;
pub const REFRESH_TOKEN: &str = "oa_refresh";
pub const REFRESH_TOKEN_DAYS: i64 = 7;
pub const CACHE_MAX_AGE: i64 = 60 * 60;
#[allow(dead_code)]
pub const APP_VERSION_URL_RELEASE: &str =
    "https://raw.githubusercontent.com/machengim/oasis/main/version.txt";
#[allow(dead_code)]
pub const APP_VERSION_URL_DEBUG: &str =
    "https://raw.githubusercontent.com/machengim/oasis/dev/version.txt";
#[allow(dead_code)]
pub const CACHE_FILE_EXTS: [&str; 3] = ["html", "js", "css"];
pub const DEFAULT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
pub const ZIP_BUFFER_SIZE: usize = 65536;
