use std::path::{Path, PathBuf};

/// Outgoing SAMP messages.
pub enum SampMessage {
    LoadImage(PathBuf),
    PointAt { ra: f64, dec: f64 },
    LoadTable(PathBuf),
}

/// Minimal SAMP client that communicates with a running hub.
///
/// SAMP uses XML-RPC over HTTP. We implement only the minimum subset needed:
/// `samp.hub.register`, `samp.hub.unregister`, `samp.hub.callAll` for
/// `image.load.fits` and `coord.pointAt.sky`, and `samp.hub.pullCallsAll`
/// for receiving incoming messages.
pub struct SampClient {
    hub_url: String,
    private_key: String,
    #[allow(dead_code)]
    pub_id: String,
    pub connected: bool,
}

impl SampClient {
    /// Discover and connect to the running SAMP hub via the `~/.samp` lockfile.
    pub fn connect() -> anyhow::Result<Self> {
        let lockfile = dirs::home_dir()
            .map(|h| h.join(".samp"))
            .ok_or_else(|| anyhow::anyhow!("cannot find home directory"))?;

        let content = std::fs::read_to_string(&lockfile)
            .map_err(|_| anyhow::anyhow!("SAMP lockfile not found at {:?}; is a hub running?", lockfile))?;

        let hub_url = content.lines()
            .find(|l| l.starts_with("samp.hub.xmlrpc.url="))
            .and_then(|l| l.strip_prefix("samp.hub.xmlrpc.url="))
            .ok_or_else(|| anyhow::anyhow!("SAMP hub URL not found in lockfile"))?
            .trim()
            .to_owned();

        // Register with the hub
        let (private_key, pub_id) = xmlrpc_register(&hub_url)?;

        log::info!("SAMP: connected to hub at {hub_url} (pub_id={pub_id})");

        Ok(Self {
            hub_url,
            private_key,
            pub_id,
            connected: true,
        })
    }

    /// Disconnect from the hub.
    pub fn disconnect(&mut self) {
        if !self.connected {
            return;
        }
        let _ = xmlrpc_call(&self.hub_url, "samp.hub.unregister", &[xml_string(&self.private_key)]);
        self.connected = false;
        log::info!("SAMP: disconnected");
    }

    /// Send a FITS image to all connected clients.
    pub fn send_image(&self, path: &Path) -> anyhow::Result<()> {
        let url = format!("file://{}", path.to_string_lossy());
        let msg = xml_struct(&[
            ("samp.mtype", xml_string("image.load.fits")),
            ("samp.params", xml_struct(&[
                ("url", xml_string(&url)),
                ("name", xml_string(&path.file_name().unwrap_or_default().to_string_lossy())),
            ])),
        ]);
        xmlrpc_call(
            &self.hub_url,
            "samp.hub.notifyAll",
            &[xml_string(&self.private_key), msg],
        )?;
        Ok(())
    }

    /// Broadcast the current cursor sky position to connected clients.
    pub fn send_point_at(&self, ra: f64, dec: f64) -> anyhow::Result<()> {
        let msg = xml_struct(&[
            ("samp.mtype", xml_string("coord.pointAt.sky")),
            ("samp.params", xml_struct(&[
                ("ra", xml_string(&ra.to_string())),
                ("dec", xml_string(&dec.to_string())),
            ])),
        ]);
        xmlrpc_call(
            &self.hub_url,
            "samp.hub.notifyAll",
            &[xml_string(&self.private_key), msg],
        )?;
        Ok(())
    }

    /// Poll for incoming messages and return them.
    pub fn poll_messages(&mut self) -> Vec<SampMessage> {
        if !self.connected {
            return Vec::new();
        }

        let result = xmlrpc_call(
            &self.hub_url,
            "samp.hub.pullCallsAll",
            &[xml_string(&self.private_key), xml_string("0")],
        );

        match result {
            Err(_) => Vec::new(),
            Ok(resp) => parse_incoming_messages(&resp),
        }
    }
}

impl Drop for SampClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

// ── XML-RPC helpers ──────────────────────────────────────────────────────────

fn xml_string(s: &str) -> String {
    format!("<value><string>{s}</string></value>")
}

fn xml_struct(fields: &[(&str, String)]) -> String {
    let members: String = fields.iter().map(|(k, v)| {
        format!("<member><name>{k}</name>{v}</member>")
    }).collect();
    format!("<value><struct>{members}</struct></value>")
}

fn xmlrpc_register(hub_url: &str) -> anyhow::Result<(String, String)> {
    let samp_info = xml_struct(&[
        ("samp.name", xml_string("fits-view")),
        ("samp.description.text", xml_string("FITS viewer")),
        ("samp.icon.url", xml_string("")),
    ]);

    let body = format!(
        r#"<?xml version="1.0"?><methodCall><methodName>samp.hub.register</methodName><params><param>{samp_info}</param></params></methodCall>"#
    );

    let resp = http_post(hub_url, &body)?;
    let private_key = extract_string_field(&resp, "samp.private-key").unwrap_or_default();
    let pub_id = extract_string_field(&resp, "samp.self-id").unwrap_or_default();
    Ok((private_key, pub_id))
}

fn xmlrpc_call(hub_url: &str, method: &str, params: &[String]) -> anyhow::Result<String> {
    let param_xml: String = params.iter()
        .map(|p| format!("<param>{p}</param>"))
        .collect();
    let body = format!(
        r#"<?xml version="1.0"?><methodCall><methodName>{method}</methodName><params>{param_xml}</params></methodCall>"#
    );
    http_post(hub_url, &body)
}

fn http_post(url: &str, body: &str) -> anyhow::Result<String> {
    use std::io::{Read, Write};
    let parsed = url::Url::parse(url)
        .map_err(|e| anyhow::anyhow!("bad URL {url}: {e}"))?;

    let host = parsed.host_str().unwrap_or("localhost");
    let port = parsed.port().unwrap_or(80);
    let path = if parsed.path().is_empty() { "/" } else { parsed.path() };

    let mut stream = std::net::TcpStream::connect((host, port))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;

    let request = format!(
        "POST {path} HTTP/1.0\r\nHost: {host}\r\nContent-Type: text/xml\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes())?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    // Strip HTTP headers
    if let Some(idx) = response.find("\r\n\r\n") {
        Ok(response[idx + 4..].to_owned())
    } else {
        Ok(response)
    }
}

fn extract_string_field(xml: &str, field: &str) -> Option<String> {
    let search = format!("<name>{field}</name>");
    let start = xml.find(&search)? + search.len();
    let rest = &xml[start..];
    let val_start = rest.find("<string>")? + "<string>".len();
    let val_end = rest[val_start..].find("</string>")?;
    Some(rest[val_start..val_start + val_end].to_owned())
}

fn parse_incoming_messages(xml: &str) -> Vec<SampMessage> {
    let mut messages = Vec::new();

    if xml.contains("image.load.fits") {
        if let Some(url) = extract_string_field(xml, "url") {
            let path = url.strip_prefix("file://").unwrap_or(&url);
            messages.push(SampMessage::LoadImage(PathBuf::from(path)));
        }
    }

    if xml.contains("coord.pointAt.sky") {
        if let (Some(ra_s), Some(dec_s)) = (
            extract_string_field(xml, "ra"),
            extract_string_field(xml, "dec"),
        ) {
            if let (Ok(ra), Ok(dec)) = (ra_s.parse::<f64>(), dec_s.parse::<f64>()) {
                messages.push(SampMessage::PointAt { ra, dec });
            }
        }
    }

    messages
}

// Minimal URL parsing (avoid extra dep for just one use)
mod url {
    pub struct Url {
        pub host: Option<String>,
        pub port: Option<u16>,
        pub path: String,
    }

    impl Url {
        pub fn parse(s: &str) -> Result<Self, &'static str> {
            // Expect http://host:port/path
            let s = s.strip_prefix("http://").ok_or("not http")?;
            let (authority, path) = if let Some(idx) = s.find('/') {
                (&s[..idx], &s[idx..])
            } else {
                (s, "/")
            };
            let (host, port) = if let Some(idx) = authority.rfind(':') {
                let port = authority[idx + 1..].parse::<u16>().ok();
                (Some(authority[..idx].to_owned()), port)
            } else {
                (Some(authority.to_owned()), None)
            };
            Ok(Url { host, port, path: path.to_owned() })
        }

        pub fn host_str(&self) -> Option<&str> { self.host.as_deref() }
        pub fn port(&self) -> Option<u16> { self.port }
        pub fn path(&self) -> &str { &self.path }
    }
}
