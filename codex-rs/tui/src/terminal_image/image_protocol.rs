use anyhow::Context;
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use base64::Engine as _;
use base64::engine::general_purpose;
use crossterm::cursor::MoveTo;
use crossterm::queue;
use ratatui::layout::Rect;

use crate::terminal_capabilities::images::wrap_graphics_command;
const ESC: &str = "\x1b";
const ST: &str = "\x1b\\";
const KITTY_CHUNK_SIZE: usize = 4096;
pub(crate) use crate::terminal_capabilities::images::ImageProtocol;
pub(crate) use crate::terminal_capabilities::images::ImageSupport;
#[cfg(test)]
pub(crate) use crate::terminal_capabilities::images::ImageUnsupportedReason;
pub(crate) use crate::terminal_capabilities::images::detect_image_support;
pub fn kitty_delete_image(image_id: u32) -> String {
    wrap_graphics_command(&format!("{ESC}_Ga=d,d=I,i={image_id},q=2;{ST}"))
}

#[derive(Clone, Copy)]
pub(crate) enum KittyPlacement {
    Cursor(u32),
    Virtual(u32),
}

impl KittyPlacement {
    pub(crate) fn draw(
        self,
        writer: &mut impl Write,
        area: Rect,
        payload: &[u8],
    ) -> std::io::Result<()> {
        queue!(writer, MoveTo(area.x, area.y))?;
        writer.write_all(payload)?;
        if let KittyPlacement::Virtual(id) = self {
            // tmux moves these cells with the pane. Kitty inherits subsequent columns in each row.
            let [_, r, g, b] = id.to_be_bytes();
            write!(writer, "\x1b[38;2;{r};{g};{b}m")?;
            let remaining_cells = "\u{10eeee}".repeat(usize::from(area.width.saturating_sub(1)));
            for (y, mark) in (area.y..area.bottom()).zip([
                '\u{305}', '\u{30d}', '\u{30e}', '\u{310}', '\u{312}', '\u{33d}',
            ]) {
                queue!(writer, MoveTo(area.x, y))?;
                write!(writer, "\u{10eeee}{mark}{remaining_cells}")?;
            }
            writer.write_all(b"\x1b[39m")?;
        }
        Ok(())
    }
}

pub(crate) fn kitty_move_image(id: u32, area: ratatui::layout::Rect) -> String {
    let (columns, rows) = (area.width, area.height);
    // Lowercase i removes placements but keeps the uploaded pixel data.
    wrap_graphics_command(&format!(
        "{ESC}_Ga=d,d=i,i={id},q=2;{ST}{ESC}_Ga=p,i={id},c={columns},r={rows},C=1,q=2;{ST}"
    ))
}

pub fn kitty_transmit_png_with_id(
    path: &Path,
    columns: u16,
    rows: u16,
    placement: KittyPlacement,
) -> Result<String> {
    let png = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let payload = general_purpose::STANDARD.encode(png);
    let chunks = payload
        .as_bytes()
        .chunks(KITTY_CHUNK_SIZE)
        .collect::<Vec<_>>();

    let mut command = String::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let chunk = std::str::from_utf8(chunk).context("base64 payload is not valid UTF-8")?;
        let has_more = index + 1 < chunks.len();
        let more_flag = u8::from(has_more);
        if index == 0 {
            let image_id = kitty_image_id_arg(placement);
            command.push_str(&format!(
                "{ESC}_Ga=T,t=d,f=100,c={columns},r={rows},q=2{image_id},m={more_flag},C=1;{chunk}{ST}",
            ));
        } else {
            command.push_str(&format!("{ESC}_Gm={more_flag},q=2;{chunk}{ST}"));
        }
    }

    Ok(wrap_graphics_command(&command))
}

pub fn kitty_transmit_png_file_with_id(
    path: &Path,
    columns: u16,
    rows: u16,
    placement: KittyPlacement,
) -> Result<String> {
    let path = path
        .canonicalize()
        .with_context(|| format!("canonicalize {}", path.display()))?;
    let payload = general_purpose::STANDARD.encode(path.to_string_lossy().as_bytes());
    let image_id = kitty_image_id_arg(placement);
    let command =
        format!("{ESC}_Ga=T,t=f,f=100,c={columns},r={rows},q=2{image_id},C=1;{payload}{ST}");

    Ok(wrap_graphics_command(&command))
}

fn kitty_image_id_arg(placement: KittyPlacement) -> String {
    match placement {
        KittyPlacement::Cursor(id) => format!(",i={id}"),
        KittyPlacement::Virtual(id) => format!(",i={id},U=1"),
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    use crate::terminal_capabilities::images::tests::EnvVarGuard;

    #[test]
    #[serial]
    fn kitty_png_transmission_encodes_inline_data() {
        let _guard = EnvVarGuard::new("TMUX", /*value*/ None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frame.png");
        fs::write(&path, b"png").unwrap();

        let command = kitty_transmit_png_with_id(
            &path,
            /*columns*/ 4,
            /*rows*/ 3,
            KittyPlacement::Cursor(7),
        )
        .unwrap();

        assert!(command.starts_with("\x1b_Ga=T,t=d,f=100,c=4,r=3,q=2,i=7,m=0,C=1;"));
        assert!(command.contains("cG5n"));
        assert!(command.ends_with("\x1b\\"));

        // Base64 encoding expands this payload to three chunks.
        fs::write(&path, vec![0; 2 * KITTY_CHUNK_SIZE]).unwrap();
        let command = kitty_transmit_png_with_id(
            &path,
            /*columns*/ 4,
            /*rows*/ 3,
            KittyPlacement::Cursor(7),
        )
        .unwrap();
        let headers = command
            .split("\x1b_G")
            .skip(1)
            .map(|chunk| chunk.split_once(';').unwrap().0)
            .collect::<Vec<_>>();
        insta::assert_snapshot!(headers.join("\n"), @r"
        a=T,t=d,f=100,c=4,r=3,q=2,i=7,m=1,C=1
        m=1,q=2
        m=0,q=2
        ");
    }

    #[test]
    #[serial]
    fn tmux_passthrough_wraps_and_escapes_control_sequence() {
        let _guard = EnvVarGuard::new("TMUX", Some("session"));
        assert_eq!(
            wrap_graphics_command("\x1b_Gx;\x1b\\"),
            "\x1bPtmux;\x1b\x1b_Gx;\x1b\x1b\\\x1b\\"
        );
    }

    #[test]
    #[serial]
    fn kitty_file_png_transmission_encodes_local_file_reference() {
        let _guard = EnvVarGuard::new("TMUX", /*value*/ None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frame.png");
        fs::write(&path, b"png").unwrap();

        let command = kitty_transmit_png_file_with_id(
            &path,
            /*columns*/ 4,
            /*rows*/ 3,
            KittyPlacement::Cursor(7),
        )
        .unwrap();
        let path = path.canonicalize().unwrap();
        let payload = general_purpose::STANDARD.encode(path.to_string_lossy().as_bytes());

        assert_eq!(
            command,
            format!("\x1b_Ga=T,t=f,f=100,c=4,r=3,q=2,i=7,C=1;{payload}\x1b\\")
        );
    }
}
