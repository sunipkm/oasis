use crate::util;
use crate::util::constants::DEFAULT_IP;
use anyhow::Result as AnyResult;
use std::cmp::Ordering;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

#[derive(Debug)]
pub struct ServerConfig {
    pub ip: IpAddr,
    pub port: u16,
    pub certs: Option<String>,
    pub key: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            ip: DEFAULT_IP,
            port: 8000,
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
                let mut content = line.trim();

                content = content.split('#').next().unwrap_or(content).trim();

                if content.is_empty() {
                    continue;
                }

                let parts: Vec<&str> = content.split('=').map(|e| e.trim()).collect();
                if parts.len() != 2 {
                    return Err(malform);
                }

                match parts[0].to_lowercase().as_str() {
                    "ip" => server.ip = IpAddr::from_str(parts[1].trim())?,
                    "port" => server.port = parts[1].trim().parse()?,
                    "certs" => server.certs = Some(parts[1].trim().to_string()),
                    "key" => server.key = Some(parts[1].trim().to_string()),
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

#[allow(dead_code)]
struct LocalIpRange {
    start: IpAddr,
    end: IpAddr,
}

impl LocalIpRange {
    #[allow(dead_code)]
    fn new(ips: [u8; 4], ipe: [u8; 4]) -> Self {
        let start = IpAddr::V4(Ipv4Addr::new(ips[0], ips[1], ips[2], ips[3]));
        let end = IpAddr::V4(Ipv4Addr::new(ipe[0], ipe[1], ipe[2], ipe[3]));

        LocalIpRange { start, end }
    }
}

pub fn show(config: &ServerConfig) -> AnyResult<()> {
    let mut ips = vec![];
    if config.ip != DEFAULT_IP {
        ips.push(config.ip);
    } else {
        match std::env::consts::OS {
            "linux" => ips = retrieve_ip_linux(),
            "macos" | "windows" => ips = retrieve_ip_win_mac(),
            _ => {}
        };
    }

    if ips.len() == 1 {
        println!("Server running on {}:{}", ips[0], config.port);
    } else {
        println!(
            "Cannot detect local IP automatically, please visit your server via its ip and port {}",
            config.port
        );
        println!("You can also specify them in the config file");
    }

    Ok(())
}

fn retrieve_ip_linux() -> Vec<IpAddr> {
    let mut ips = vec![];

    if let Ok(ip) = local_ip_address::local_ip() {
        ips.push(ip);
    }

    ips
}

fn retrieve_ip_win_mac() -> Vec<IpAddr> {
    let ranges = vec![
    LocalIpRange::new([192, 168, 0, 0], [192, 168, 255, 255]),
    LocalIpRange::new([172, 16, 0, 0], [172, 31, 255, 255]),
    LocalIpRange::new([10, 0, 0, 0], [10, 255, 255, 255])];

    // the name is not sure, it could be "wlan" or "以太网" on some devices.
    // let names = vec!["ethernet", "wi-fi", "en0"];
    let network_interfaces = local_ip_address::list_afinet_netifas().unwrap();
    let mut ips = vec![];
    for (_name, ip) in network_interfaces.iter() {
        if !ip.is_ipv4() {
            continue;
        }

        for range in ranges.iter() {
            if ip.cmp(&range.start) == Ordering::Greater && ip.cmp(&range.end) == Ordering::Less
            // && names.contains(&name.to_lowercase().as_str())
            {
                ips.push(*ip);
            }
        }
    }

    ips
}
