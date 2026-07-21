//! Optional Kitty-graphics startup art.
//!
//! Herdr deliberately gates its graphics proxy behind
//! `[experimental].kitty_graphics`, so the switcher is conservative too: the
//! animated PNG frames are only sent when both the outer terminal and Herdr say
//! they support the protocol. The ordinary ratatui pixel cat remains the
//! universal fallback.

use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::layout::Rect;

const FRAME_BYTES: [&[u8]; 6] = [
    include_bytes!("../assets/images/cat-typing-frames/frame-00.png"),
    include_bytes!("../assets/images/cat-typing-frames/frame-01.png"),
    include_bytes!("../assets/images/cat-typing-frames/frame-02.png"),
    include_bytes!("../assets/images/cat-typing-frames/frame-03.png"),
    include_bytes!("../assets/images/cat-typing-frames/frame-04.png"),
    include_bytes!("../assets/images/cat-typing-frames/frame-05.png"),
];

const IMAGE_COLS: u16 = 20;
const IMAGE_ROWS: u16 = 11;
const IMAGE_WIDTH: u32 = 192;
const IMAGE_HEIGHT: u32 = 208;
const CONTENT_ROWS: u16 = IMAGE_ROWS + 3;
const MIN_WIDTH: u16 = 40;
const MIN_HEIGHT: u16 = 16;
const PAYLOAD_CHUNK: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Placement {
    /// Zero-based terminal column.
    pub column: u16,
    /// Zero-based terminal row.
    pub row: u16,
    pub columns: u16,
    pub rows: u16,
}

/// Center the image plus the two status rows beneath it. Small panes use the
/// compact ASCII cat instead of asking the terminal to crop a graphic.
pub fn placement(area: Rect) -> Option<Placement> {
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        return None;
    }
    Some(Placement {
        column: area.x + area.width.saturating_sub(IMAGE_COLS) / 2,
        row: area.y + area.height.saturating_sub(CONTENT_ROWS) / 2,
        columns: IMAGE_COLS,
        rows: IMAGE_ROWS,
    })
}

enum Backend {
    /// Herdr's acknowledged API is preferred inside managed panes. Unlike raw
    /// terminal escape sequences, a rejected frame can reliably fall back.
    Herdr(HerdrGraphics),
    /// Direct terminal rendering remains useful when the binary is launched
    /// outside Herdr in a compatible terminal.
    Kitty { image_id: u32 },
}

pub struct Splash {
    backend: Option<Backend>,
    uploaded: bool,
    displayed_frame: Option<usize>,
    placed_at: Option<Placement>,
}

impl Splash {
    pub fn new() -> Self {
        let pid = std::process::id() & 0x00ff_ffff;
        let backend = if !environment_supports_kitty() {
            None
        } else if env::var_os("HERDR_ENV").is_some() {
            HerdrGraphics::connect().ok().map(Backend::Herdr)
        } else {
            Some(Backend::Kitty {
                // A stable prefix makes terminal captures recognizable while
                // the process id prevents two live panes from sharing an ID.
                image_id: 0x4700_0000 | pid,
            })
        };
        Splash {
            backend,
            uploaded: false,
            displayed_frame: None,
            placed_at: None,
        }
    }

    pub fn can_show(&self, area: Rect) -> bool {
        self.backend.is_some() && placement(area).is_some()
    }

    /// Publish the current PNG through Herdr's acknowledged graphics API, or
    /// directly as an ordinary static Kitty image outside Herdr. Any failure
    /// disables graphics for this run, so the caller immediately redraws the
    /// ASCII fallback instead of leaving an empty image slot.
    pub fn show(&mut self, area: Rect, frame: usize) -> bool {
        let Some(place) = placement(area).filter(|_| self.backend.is_some()) else {
            self.clear();
            return false;
        };
        if self.try_show(place, frame).is_err() {
            self.clear();
            self.backend = None;
            return false;
        }
        true
    }

    fn try_show(&mut self, place: Placement, frame: usize) -> io::Result<()> {
        let frame = frame % FRAME_BYTES.len();
        match self.backend.as_mut() {
            Some(Backend::Herdr(client)) => {
                if self.displayed_frame != Some(frame) || self.placed_at != Some(place) {
                    client.set_frame(FRAME_BYTES[frame], place)?;
                    self.uploaded = true;
                    self.displayed_frame = Some(frame);
                    self.placed_at = Some(place);
                }
                Ok(())
            }
            Some(Backend::Kitty { image_id }) => {
                let stdout = io::stdout();
                let mut out = stdout.lock();
                if self.displayed_frame != Some(frame) {
                    upload_frame(&mut out, *image_id, FRAME_BYTES[frame])?;
                    self.uploaded = true;
                    self.displayed_frame = Some(frame);
                    // Replacing data for a fixed Kitty image ID removes its
                    // existing placements, so every frame must be re-placed.
                    self.placed_at = None;
                }
                if self.placed_at != Some(place) {
                    place_image(&mut out, *image_id, place)?;
                    self.placed_at = Some(place);
                }
                out.flush()
            }
            None => Err(io::Error::other("startup graphics are unavailable")),
        }
    }

    /// Remove both the placement and its stored frames before the picker or
    /// shell becomes visible. Cleanup is best effort because a closed terminal
    /// must never turn a successful cancel into an error.
    pub fn clear(&mut self) {
        if !self.uploaded {
            return;
        }
        match self.backend.as_mut() {
            Some(Backend::Herdr(client)) => {
                let _ = client.clear();
            }
            Some(Backend::Kitty { image_id }) => {
                let stdout = io::stdout();
                let mut out = stdout.lock();
                let _ = delete_image(&mut out, *image_id).and_then(|()| out.flush());
            }
            None => {}
        }
        self.uploaded = false;
        self.displayed_frame = None;
        self.placed_at = None;
    }
}

struct HerdrGraphics {
    writer: UnixStream,
    reader: BufReader<UnixStream>,
    pane_id: String,
    sequence: u32,
}

impl HerdrGraphics {
    fn connect() -> io::Result<Self> {
        let socket = env::var_os("HERDR_SOCKET_PATH")
            .ok_or_else(|| io::Error::other("HERDR_SOCKET_PATH is not set"))?;
        let pane_id =
            env::var("HERDR_PANE_ID").map_err(|_| io::Error::other("HERDR_PANE_ID is not set"))?;
        let writer = UnixStream::connect(PathBuf::from(socket))?;
        Self::from_stream(writer, pane_id)
    }

    fn from_stream(writer: UnixStream, pane_id: String) -> io::Result<Self> {
        writer.set_write_timeout(Some(Duration::from_millis(80)))?;
        writer.set_read_timeout(Some(Duration::from_millis(80)))?;
        let reader = BufReader::new(writer.try_clone()?);
        Ok(HerdrGraphics {
            writer,
            reader,
            pane_id,
            sequence: 0,
        })
    }

    fn set_frame(&mut self, frame: &[u8], place: Placement) -> io::Result<()> {
        let params = serde_json::json!({
            "pane_id": self.pane_id,
            "format": "png",
            "image_width": IMAGE_WIDTH,
            "image_height": IMAGE_HEIGHT,
            "data_base64": base64(frame),
            "placement": {
                "viewport_col": place.column,
                "viewport_row": place.row,
                "grid_cols": place.columns,
                "grid_rows": place.rows,
            },
        });
        self.request("pane.graphics.set", params)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.request(
            "pane.graphics.clear",
            serde_json::json!({ "pane_id": self.pane_id }),
        )
    }

    fn request(&mut self, method: &str, params: serde_json::Value) -> io::Result<()> {
        self.sequence = self.sequence.wrapping_add(1);
        let id = format!("ghq-splash-{}", self.sequence);
        let request = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });
        serde_json::to_writer(&mut self.writer, &request).map_err(io::Error::other)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;

        let mut response = String::new();
        if self.reader.read_line(&mut response)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Herdr closed the graphics API connection",
            ));
        }
        let response: serde_json::Value =
            serde_json::from_str(&response).map_err(io::Error::other)?;
        if response.get("id").and_then(serde_json::Value::as_str) != Some(id.as_str()) {
            return Err(io::Error::other("Herdr returned a mismatched response"));
        }
        if response.get("result").is_none() {
            let message = response
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Herdr rejected the graphics frame");
            return Err(io::Error::other(message.to_owned()));
        }
        Ok(())
    }
}

impl Drop for Splash {
    fn drop(&mut self) {
        self.clear();
    }
}

fn environment_supports_kitty() -> bool {
    if !outer_terminal_supports_kitty() {
        return false;
    }
    if env::var_os("HERDR_ENV").is_none() {
        return true;
    }
    herdr_config_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .is_some_and(|config| experimental_kitty_enabled(&config))
}

fn outer_terminal_supports_kitty() -> bool {
    if env::var_os("KITTY_WINDOW_ID").is_some() {
        return true;
    }
    let term = env::var("TERM").unwrap_or_default().to_ascii_lowercase();
    if term.contains("kitty") {
        return true;
    }
    matches!(
        env::var("TERM_PROGRAM")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "ghostty" | "kitty" | "wezterm"
    )
}

fn herdr_config_path() -> Option<PathBuf> {
    if let Some(root) = env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(root).join("herdr/config.toml"));
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/herdr/config.toml"))
}

/// Read only Herdr's one boolean. This intentionally is not a general TOML
/// parser: malformed, absent, or differently-scoped keys all mean disabled.
fn experimental_kitty_enabled(config: &str) -> bool {
    let mut experimental = false;
    for raw in config.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') {
            experimental = line == "[experimental]";
            continue;
        }
        if !experimental {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "kitty_graphics" {
            return value.trim() == "true";
        }
    }
    false
}

fn upload_frame(out: &mut impl Write, image_id: u32, frame: &[u8]) -> io::Result<()> {
    transmit(out, &format!("a=t,t=d,f=100,i={image_id},q=2"), frame)
}

fn place_image(out: &mut impl Write, image_id: u32, place: Placement) -> io::Result<()> {
    write!(out, "\x1b[{};{}H", place.row + 1, place.column + 1)?;
    command(
        out,
        &format!(
            "a=p,i={image_id},p=1,c={},r={},C=1,q=2",
            place.columns, place.rows
        ),
    )
}

fn delete_image(out: &mut impl Write, image_id: u32) -> io::Result<()> {
    command(out, &format!("a=d,d=I,i={image_id},q=2"))
}

fn command(out: &mut impl Write, control: &str) -> io::Result<()> {
    write!(out, "\x1b_G{control}\x1b\\")
}

fn transmit(out: &mut impl Write, control: &str, bytes: &[u8]) -> io::Result<()> {
    let encoded = base64(bytes);
    let chunks = encoded.as_bytes().chunks(PAYLOAD_CHUNK);
    let total = chunks.len();
    for (index, chunk) in chunks.enumerate() {
        let more = usize::from(index + 1 < total);
        if index == 0 {
            write!(out, "\x1b_G{control},m={more};")?;
        } else {
            write!(out, "\x1b_Gm={more};")?;
        }
        out.write_all(chunk)?;
        out.write_all(b"\x1b\\")?;
    }
    Ok(())
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        encoded.push(TABLE[(a >> 2) as usize] as char);
        encoded.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            TABLE[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            TABLE[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_vectors() {
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), encoded);
        }
    }

    #[test]
    fn herdr_setting_must_be_true_in_the_experimental_section() {
        assert!(experimental_kitty_enabled(
            "[ui]\nkitty_graphics = false\n\n[experimental]\nkitty_graphics = true # enabled\n"
        ));
        assert!(!experimental_kitty_enabled(
            "[ui]\nkitty_graphics = true\n\n[experimental]\nkitty_graphics = false\n"
        ));
        assert!(!experimental_kitty_enabled(
            "[experimental]\n# kitty_graphics = true\n"
        ));
        assert!(!experimental_kitty_enabled("kitty_graphics = true\n"));
    }

    #[test]
    fn placement_centers_the_image_and_rejects_small_panes() {
        assert_eq!(
            placement(Rect::new(0, 0, 80, 24)),
            Some(Placement {
                column: 30,
                row: 5,
                columns: 20,
                rows: 11,
            })
        );
        assert!(placement(Rect::new(0, 0, 39, 24)).is_none());
        assert!(placement(Rect::new(0, 0, 80, 15)).is_none());
    }

    #[test]
    fn protocol_retransmits_static_frames_places_and_deletes_image() {
        let place = placement(Rect::new(0, 0, 80, 24)).expect("large test area");
        let mut out = Vec::new();
        upload_frame(&mut out, 77, FRAME_BYTES[0]).expect("first frame");
        place_image(&mut out, 77, place).expect("first placement");
        upload_frame(&mut out, 77, FRAME_BYTES[1]).expect("second frame");
        place_image(&mut out, 77, place).expect("second placement");
        delete_image(&mut out, 77).expect("delete");
        let protocol = String::from_utf8(out).expect("protocol is ASCII + base64");
        assert_eq!(protocol.matches("a=t,t=d,f=100,i=77").count(), 2);
        assert!(!protocol.contains("N=1"));
        assert!(!protocol.contains("a=f"));
        assert!(!protocol.contains("a=a"));
        assert_eq!(protocol.matches("\x1b[6;31H").count(), 2);
        assert_eq!(protocol.matches("a=p,i=77,p=1,c=20,r=11,C=1").count(), 2);
        assert!(protocol.contains("a=d,d=I,i=77"));
    }

    #[test]
    fn herdr_backend_sends_frames_and_requires_success_responses() {
        let (client, server) = UnixStream::pair().expect("local socket pair");
        let server_thread = std::thread::spawn(move || {
            let mut writer = server.try_clone().expect("clone server socket");
            let mut reader = BufReader::new(server);
            for expected_method in ["pane.graphics.set", "pane.graphics.clear"] {
                let mut line = String::new();
                reader.read_line(&mut line).expect("request line");
                let request: serde_json::Value = serde_json::from_str(&line).expect("request JSON");
                assert_eq!(request["method"], expected_method);
                assert_eq!(request["params"]["pane_id"], "pane-test");
                if expected_method == "pane.graphics.set" {
                    assert_eq!(request["params"]["format"], "png");
                    assert_eq!(request["params"]["image_width"], IMAGE_WIDTH);
                    assert_eq!(request["params"]["image_height"], IMAGE_HEIGHT);
                    assert_eq!(request["params"]["placement"]["viewport_col"], 30);
                    assert_eq!(request["params"]["placement"]["viewport_row"], 5);
                    assert_eq!(request["params"]["placement"]["grid_cols"], 20);
                    assert_eq!(request["params"]["placement"]["grid_rows"], 11);
                    assert_eq!(request["params"]["data_base64"], base64(FRAME_BYTES[0]));
                }
                let response = serde_json::json!({
                    "id": request["id"],
                    "result": { "type": "ok" },
                });
                writeln!(writer, "{response}").expect("response line");
                writer.flush().expect("flush response");
            }
        });

        let mut graphics =
            HerdrGraphics::from_stream(client, "pane-test".into()).expect("graphics client");
        let place = placement(Rect::new(0, 0, 80, 24)).expect("large test area");
        graphics
            .set_frame(FRAME_BYTES[0], place)
            .expect("acknowledged frame");
        graphics.clear().expect("acknowledged clear");
        server_thread.join().expect("server thread");
    }

    #[test]
    fn embedded_frames_are_pngs_with_a_transparency_chunk() {
        for frame in FRAME_BYTES {
            assert_eq!(&frame[..8], b"\x89PNG\r\n\x1a\n");
            assert!(
                frame.windows(4).any(|chunk| chunk == b"tRNS"),
                "embedded startup frame must carry indexed alpha"
            );
        }
    }
}
