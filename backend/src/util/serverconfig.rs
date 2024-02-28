use crate::util;
use crate::util::constants::DEFAULT_IP;
use anyhow::Result as AnyResult;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug)]
pub(crate) struct ServerConfig {
    pub ip: IpAddr,
    pub port: u16,
    pub datastor: PathBuf,
    pub dbstor: PathBuf,
    pub certs: Option<String>,
    pub key: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        let cwd = util::get_pwd();
        Self {
            ip: DEFAULT_IP,
            port: 8000,
            datastor: cwd.join("data"),
            dbstor: cwd.join("db"),
            certs: None,
            key: None,
        }
    }
}

impl ServerConfig {
    pub fn new() -> AnyResult<Self> {
        let mut server = Self::default();
        let malform = anyhow::anyhow!("Malformed config file");
        let conf_file = util::get_pwd().join("oasis.conf");

        if conf_file.exists() && conf_file.is_file() {
            let file = File::open(conf_file)?;
            let lines = BufReader::new(file).lines();
            for line in lines.flatten() {
                let content = line.trim();

                let content = content.split('#').next().unwrap_or(content).trim(); // Remove comments

                if content.is_empty() {
                    continue;
                }

                let parts: Vec<&str> = content.split('=').map(|x| x.trim()).collect();
                if parts.len() != 2 {
                    return Err(malform);
                }

                match parts[0].to_lowercase().as_str() {
                    "ip" => server.ip = IpAddr::from_str(parts[1])?,
                    "port" => server.port = parts[1].parse()?,
                    "datastor" => {
                        server.datastor = {
                            let path = PathBuf::from(parts[1]);
                            if !path.exists() {
                                return Err(anyhow::anyhow!("Invalid datastor path"));
                            }
                            path
                        }
                    }
                    "dbstor" => {
                        server.dbstor = {
                            let path = PathBuf::from(parts[1]);
                            if !path.exists() {
                                return Err(anyhow::anyhow!("Invalid dbstor path"));
                            }
                            path
                        }
                    }
                    "certs" => server.certs = Some(parts[1].to_string()),
                    "key" => server.key = Some(parts[1].to_string()),
                    _ => return Err(malform),
                }
            }
            return Ok(server);
        }
        Err(malform)
    }

    pub fn get_tls_str(&self) -> String {
        match (&self.certs, &self.key) {
            (Some(certs), Some(key)) => {
                format!("{{certs={:?},key={:?}}}", certs, key)
            }
            _ => String::new(),
        }
    }
}
