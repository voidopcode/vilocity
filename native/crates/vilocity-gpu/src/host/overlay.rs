//! A picture drawn over what is shown (the frame-rate panel of the
//! diagnostics build, the in-game menu): painted on the CPU when it changes
//! (BGRA, top-down), kept in a buffer in the card's memory in the swap
//! chain's format, and copied onto the swap chain's buffer at each present,
//! behind the title's picture: saved frames and the frame recorder see the
//! title's own. Corners may be rounded (rows copied shorter there).
//! [`TextBox`] paints lines of text in a font of 5x7 dots for it.

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::{Host, HostList, transition};

/// A character's dots, and with the space beside and below it.
const GLYPH: (u32, u32) = (5, 7);
const CELL: (u32, u32) = (6, 9);
/// The box's rim in dots.
const RIM: u32 = 3;

/// The pictures are put into one of these in turn: the card reads the others.
const UPLOADS: usize = 4;

/// Seven rows of five dots, the leftmost the highest bit.
#[rustfmt::skip]
pub(super) const fn glyph(c: u8) -> [u8; 7] {
    match c {
        b'0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        b'1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        b'2' => [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111],
        b'3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        b'4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        b'5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        b'6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        b'7' => [0b11111, 0b00001, 0b00010, 0b00010, 0b00100, 0b00100, 0b00100],
        b'8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        b'9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        b'A' => [0b00100, 0b01010, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001],
        b'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        b'C' => [0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111],
        b'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        b'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        b'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        b'G' => [0b01111, 0b10000, 0b10000, 0b10011, 0b10001, 0b10001, 0b01111],
        b'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        b'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        b'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        b'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        b'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        b'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        b'N' => [0b10001, 0b11001, 0b11001, 0b10101, 0b10011, 0b10011, 0b10001],
        b'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        b'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        b'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        b'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        b'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        b'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        b'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        b'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        b'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        b'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        b'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        b'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        b'.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        b'/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        b'-' => [0, 0, 0, 0b11111, 0, 0, 0],
        b'<' => [0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010],
        b'>' => [0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000],
        b':' => [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0],
        b'(' => [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010],
        b')' => [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000],
        b'+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        b'=' => [0, 0, 0b11111, 0, 0b11111, 0, 0],
        b' ' => [0; 7],
        _ => [0b11111, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11111],
    }
}

/// Characters in a line and lines of a box.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Layout {
    pub(super) columns: u32,
    pub(super) lines: u32,
}

impl Layout {
    /// The box's size in pixels, a dot being `scale` of them each way.
    const fn size(self, scale: u32) -> (u32, u32) {
        ((self.columns * CELL.0 + 2 * RIM) * scale, (self.lines * CELL.1 + 2 * RIM) * scale)
    }

    /// Whether a dot of the picture of `lines` is white; `inverted`: the
    /// line drawn black on white.
    fn lit(self, lines: &[String], inverted: Option<usize>, x: u32, y: u32) -> bool {
        let (Some(x), Some(y)) = (x.checked_sub(RIM), y.checked_sub(RIM)) else {
            return false;
        };
        let line = (y / CELL.1) as usize;
        if line >= self.lines as usize || x / CELL.0 >= self.columns {
            return false;
        }
        let dot = x % CELL.0 < GLYPH.0
            && y % CELL.1 < GLYPH.1
            && lines.get(line).and_then(|l| l.as_bytes().get((x / CELL.0) as usize)).is_some_and(|&c| {
                (glyph(c.to_ascii_uppercase())[(y % CELL.1) as usize] >> (GLYPH.0 - 1 - x % CELL.0)) & 1 != 0
            });
        dot != (inverted == Some(line))
    }

    /// The box as BGRA rows, white on black.
    fn painted(self, lines: &[String], inverted: Option<usize>, scale: u32) -> Vec<u8> {
        let (width, height) = self.size(scale);
        let mut out = vec![0; (width * height * 4) as usize];
        for (i, pixel) in out.chunks_exact_mut(4).enumerate() {
            let (x, y) = (i as u32 % width / scale, i as u32 / width / scale);
            let v = if self.lit(lines, inverted, x, y) { 0xFF } else { 0 };
            pixel.copy_from_slice(&[v, v, v, 0xFF]);
        }
        out
    }
}

/// Pixels to a dot for a picture that high: a 720-line screenshot of it
/// shows one and a third.
fn scale(height: u32) -> u32 {
    (height / 540).max(1)
}

/// The bytes of a pixel of the format, if it is one this knows.
fn pixel_bytes(format: DXGI_FORMAT) -> Option<u32> {
    match format {
        DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_R10G10B10A2_UNORM => Some(4),
        DXGI_FORMAT_R16G16B16A16_FLOAT => Some(8),
        _ => None,
    }
}

/// A half of `v` (0 to a few: no infinities, small values flushed to 0).
fn half(v: f32) -> u16 {
    let bits = v.to_bits();
    let exponent = ((bits >> 23) & 0xFF) as i32 - 127 + 15;
    if v <= 0.0 || exponent <= 0 {
        return 0;
    }
    ((exponent.min(30) as u32) << 10 | (bits >> 13) & 0x3FF) as u16
}

/// A BGRA picture of `width` pixels a row as rows of `pitch` bytes in
/// `format` (sRGB-encoded for the 8 and 10 bit ones, light for the halves),
/// opaque.
fn converted(bgra: &[u8], width: u32, format: DXGI_FORMAT, pitch: usize) -> Option<Vec<u8>> {
    let bytes = pixel_bytes(format)? as usize;
    let rows = bgra.len() / (width as usize * 4);
    let mut out = vec![0; pitch * rows];
    for (y, row) in bgra.chunks_exact(width as usize * 4).enumerate() {
        for (x, p) in row.chunks_exact(4).enumerate() {
            let (b, g, r) = (p[0], p[1], p[2]);
            let at = y * pitch + x * bytes;
            let to = &mut out[at..at + bytes];
            match format {
                DXGI_FORMAT_B8G8R8A8_UNORM => to.copy_from_slice(&[b, g, r, 0xFF]),
                DXGI_FORMAT_R8G8B8A8_UNORM => to.copy_from_slice(&[r, g, b, 0xFF]),
                DXGI_FORMAT_R10G10B10A2_UNORM => {
                    let ten = |c: u8| (u32::from(c) * 1023 + 127) / 255;
                    to.copy_from_slice(&(ten(r) | ten(g) << 10 | ten(b) << 20 | 3 << 30).to_le_bytes());
                }
                _ => {
                    let light = |c: u8| half((f32::from(c) / 255.0).powf(2.2));
                    for (i, h) in [light(r), light(g), light(b), half(1.0)].into_iter().enumerate() {
                        to[2 * i..2 * i + 2].copy_from_slice(&h.to_le_bytes());
                    }
                }
            }
        }
    }
    Some(out)
}

/// For a corner of `radius` pixels: how many pixels row `y` of the corner
/// (0 the outermost) leaves out at its end.
fn inset(radius: u32, y: u32) -> u32 {
    let r = radius as f32;
    let dy = r - y as f32 - 0.5;
    (r - (r * r - dy * dy).max(0.0).sqrt()).round() as u32
}

/// The buffers of a picture of that size and format.
struct Buffers {
    format: DXGI_FORMAT,
    size: (u32, u32),
    pitch: u32,
    uploads: Vec<(ID3D12Resource, *mut u8)>,
    card: ID3D12Resource,
    /// The upload written last.
    turn: usize,
    /// It is not in the card's buffer yet.
    fresh: bool,
}

// SAFETY: free-threaded resources, and memory only written under the presenter's lock.
unsafe impl Send for Buffers {}

impl Buffers {
    fn new(host: &Host, format: DXGI_FORMAT, (width, height): (u32, u32)) -> Option<Buffers> {
        let pitch = (width * pixel_bytes(format)?).next_multiple_of(256);
        let bytes = u64::from(pitch) * u64::from(height);
        let uploads = (0..UPLOADS).map(|_| host.upload_buffer(bytes)).collect::<Option<Vec<_>>>()?;
        let heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            ..Default::default()
        };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: bytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        let mut card: Option<ID3D12Resource> = None;
        // SAFETY: plain creation of a buffer.
        unsafe {
            host.device
                .CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COMMON, None, &mut card)
                .ok()?;
        }
        Some(Buffers { format, size: (width, height), pitch, uploads, card: card?, turn: 0, fresh: false })
    }

    fn write(&mut self, bgra: &[u8]) {
        let Some(picture) = converted(bgra, self.size.0, self.format, self.pitch as usize) else { return };
        self.turn = (self.turn + 1) % UPLOADS;
        // SAFETY: a mapped upload buffer of the picture's size, which the card read a second or more ago.
        unsafe { std::ptr::copy_nonoverlapping(picture.as_ptr(), self.uploads[self.turn].1, picture.len()) };
        self.fresh = true;
    }
}

/// Where a picture is shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Place {
    /// (The frame-rate panel's, in diagnostics builds.)
    #[cfg_attr(not(feature = "diagnostics"), allow(dead_code))]
    TopLeft,
    Middle,
}

/// A picture to show.
pub(super) struct Overlay {
    size: (u32, u32),
    bgra: Vec<u8>,
    radius: u32,
    changed: bool,
    buffers: Option<Buffers>,
    failed: bool,
}

impl Overlay {
    pub(super) fn new() -> Overlay {
        Overlay { size: (0, 0), bgra: Vec::new(), radius: 0, changed: false, buffers: None, failed: false }
    }

    /// The picture from here on: BGRA rows of `size`, the corners rounded by
    /// `radius` pixels.
    pub(super) fn set(&mut self, size: (u32, u32), bgra: Vec<u8>, radius: u32) {
        debug_assert_eq!(bgra.len(), (size.0 * size.1 * 4) as usize);
        (self.size, self.bgra, self.radius, self.changed) = (size, bgra, radius.min(size.0 / 2).min(size.1 / 2), true);
    }

    /// Records the copy of the picture onto `back`, a buffer of the swap
    /// chain of that size and format in the state to be copied into.
    pub(super) fn draw(
        &mut self,
        host: &Host,
        list: &HostList,
        back: &ID3D12Resource,
        (width, height): (u32, u32),
        format: DXGI_FORMAT,
        place: Place,
    ) {
        if self.bgra.is_empty() || self.failed {
            return;
        }
        // Not the title's: the frame recorder leaves it out.
        #[cfg(feature = "diagnostics")]
        let _ours = crate::replay::record::unrecorded();
        if self.buffers.as_ref().is_none_or(|b| (b.format, b.size) != (format, self.size)) {
            // (What the card may still read stays: a picture's size changes now and then.)
            std::mem::forget(self.buffers.take());
            self.buffers = Buffers::new(host, format, self.size);
            self.failed = self.buffers.is_none();
            self.changed = true;
        }
        let Some(buffers) = &mut self.buffers else { return };
        if std::mem::take(&mut self.changed) {
            buffers.write(&self.bgra);
        }
        let (bw, bh) = self.size;
        let (w, h) = (bw.min(width), bh.min(height));
        let (x, y) = match place {
            Place::TopLeft => (0, 0),
            Place::Middle => ((width - w) / 2, (height - h) / 2),
        };
        // The rows to copy: whole in the middle, shorter at rounded corners.
        let r = self.radius.min(h / 2);
        let mut parts: Vec<(u32, u32, u32)> = Vec::with_capacity(2 * r as usize + 1);
        for row in 0..r {
            let cut = inset(r, row);
            parts.push((row, row + 1, cut));
            parts.push((h - 1 - row, h - row, cut));
        }
        parts.push((r, h - r, 0));
        let list = &list.list;
        // SAFETY: an open list and live resources; the picture lies within both.
        unsafe {
            if std::mem::take(&mut buffers.fresh) {
                list.CopyResource(&buffers.card, &buffers.uploads[buffers.turn].0);
                list.ResourceBarrier(&[transition(
                    &buffers.card,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                )]);
            }
            let from = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&buffers.card),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                        Offset: 0,
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                            Format: format,
                            Width: bw,
                            Height: bh,
                            Depth: 1,
                            RowPitch: buffers.pitch,
                        },
                    },
                },
            };
            let to = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(back),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
            };
            for (top, bottom, cut) in parts {
                if top >= bottom || 2 * cut >= w {
                    continue;
                }
                let part = D3D12_BOX { left: cut, top, front: 0, right: w - cut, bottom, back: 1 };
                list.CopyTextureRegion(&to, x + cut, y + top, 0, &from, Some(&part));
            }
        }
    }
}

/// Lines of text in the font of dots, at the top left or in the middle.
#[cfg_attr(not(feature = "diagnostics"), allow(dead_code))]
pub(super) struct TextBox {
    layout: Layout,
    lines: Vec<String>,
    inverted: Option<usize>,
    /// The scale the overlay's picture was painted at, if it is current.
    painted: Option<u32>,
    overlay: Overlay,
}

#[cfg_attr(not(feature = "diagnostics"), allow(dead_code))]
impl TextBox {
    pub(super) fn new(layout: Layout) -> TextBox {
        TextBox { layout, lines: Vec::new(), inverted: None, painted: None, overlay: Overlay::new() }
    }

    /// Its text from here on (painted at the next draw if it changed).
    pub(super) fn set(&mut self, lines: Vec<String>, inverted: Option<usize>) {
        if lines != self.lines || inverted != self.inverted {
            (self.lines, self.inverted, self.painted) = (lines, inverted, None);
        }
    }

    /// As [`Overlay::draw`].
    pub(super) fn draw(
        &mut self,
        host: &Host,
        list: &HostList,
        back: &ID3D12Resource,
        size: (u32, u32),
        format: DXGI_FORMAT,
        place: Place,
    ) {
        if self.lines.is_empty() {
            return;
        }
        let scale = scale(size.1);
        if self.painted != Some(scale) {
            self.overlay.set(self.layout.size(scale), self.layout.painted(&self.lines, self.inverted, scale), 0);
            self.painted = Some(scale);
        }
        self.overlay.draw(host, list, back, size, format, place);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character the texts use has its own picture, and no two look
    /// alike.
    #[test]
    fn characters_differ() {
        let used: Vec<u8> = (b'0'..=b'9').chain(b'A'..=b'Z').chain(*b"./-<>:()+= ").collect();
        for (i, a) in used.iter().enumerate() {
            assert_ne!(glyph(*a), glyph(b'#'), "{} has a picture", *a as char);
            assert!(glyph(*a).iter().all(|row| row >> GLYPH.0 == 0));
            for b in &used[i + 1..] {
                assert_ne!(glyph(*a), glyph(*b), "{} {}", *a as char, *b as char);
            }
        }
    }

    #[test]
    fn a_box_is_painted() {
        let layout = Layout { columns: 40, lines: 9 };
        let lines = vec!["I".to_owned(), " i".to_owned()];
        let (white, black) = ([0xFF; 4], [0, 0, 0, 0xFF]);
        for scale in [1, 2, 4] {
            let (width, height) = layout.size(scale);
            let picture = layout.painted(&lines, None, scale);
            assert_eq!(picture.len(), (width * height * 4) as usize);
            let at = |x: u32, y: u32| &picture[((y * width + x) * 4) as usize..][..4];
            // The rim is black, the first letter's top row lit in its middle.
            assert_eq!(at(0, 0), black);
            assert_eq!(at((RIM + 2) * scale, RIM * scale), white);
            assert_eq!(at((RIM + 2) * scale + scale - 1, RIM * scale + scale - 1), white);
            assert_eq!(at(RIM * scale, RIM * scale), black);
            // Small letters are shown as capitals, a line and a cell on.
            assert_eq!(at((RIM + CELL.0 + 2) * scale, (RIM + CELL.1) * scale), white);
            // Nothing past a line's end.
            assert_eq!(at((RIM + CELL.0 * 2 + 2) * scale, (RIM + CELL.1) * scale), black);
        }
    }

    /// An inverted line is white where its letters are not, and black in them;
    /// the rim and the other lines stay as they are.
    #[test]
    fn an_inverted_line_is_black_on_white() {
        let layout = Layout { columns: 4, lines: 2 };
        let lines = vec!["I".to_owned(), "I".to_owned()];
        assert!(layout.lit(&lines, Some(1), RIM + 4 * CELL.0 - 1, RIM + CELL.1));
        assert!(!layout.lit(&lines, Some(1), RIM + 2, RIM + CELL.1));
        assert!(layout.lit(&lines, Some(1), RIM + 2, RIM));
        assert!(!layout.lit(&lines, Some(1), RIM + 4 * CELL.0 - 1, RIM));
        assert!(!layout.lit(&lines, Some(1), 0, 0), "the rim");
    }

    /// A pixel in each format the swap chains here use, at the place of
    /// its row's pitch.
    #[test]
    fn pixels_are_converted_to_the_swap_chains_format() {
        // Two rows of one pixel: an orange (B 0x20, G 0x80, R 0xFF), then black.
        let bgra = [0x20, 0x80, 0xFF, 0, 0, 0, 0, 0];
        let one = |format, pitch| converted(&bgra, 1, format, pitch).unwrap();
        assert_eq!(one(DXGI_FORMAT_B8G8R8A8_UNORM, 256)[..4], [0x20, 0x80, 0xFF, 0xFF]);
        assert_eq!(one(DXGI_FORMAT_R8G8B8A8_UNORM, 256)[..4], [0xFF, 0x80, 0x20, 0xFF]);
        assert_eq!(one(DXGI_FORMAT_R8G8B8A8_UNORM, 256)[256..260], [0, 0, 0, 0xFF], "the second row");
        let ten = u32::from_le_bytes(one(DXGI_FORMAT_R10G10B10A2_UNORM, 256)[..4].try_into().unwrap());
        assert_eq!((ten & 0x3FF, ten >> 10 & 0x3FF, ten >> 20 & 0x3FF, ten >> 30), (1023, 514, 128, 3));
        let halves = one(DXGI_FORMAT_R16G16B16A16_FLOAT, 256);
        let h = |i: usize| u16::from_le_bytes([halves[2 * i], halves[2 * i + 1]]);
        assert_eq!((h(0), h(3)), (0x3C00, 0x3C00), "full red and an opaque alpha are 1.0");
        assert!(h(2) > 0 && h(2) < h(1) && h(1) < 0x3C00, "blue below green below 1.0, as light");
        assert!(converted(&bgra, 1, DXGI_FORMAT_R32G32B32A32_FLOAT, 256).is_none());
    }

    #[test]
    fn halves_are_right() {
        assert_eq!(half(0.0), 0);
        assert_eq!(half(1.0), 0x3C00);
        assert_eq!(half(0.5), 0x3800);
        assert_eq!(half(2.0), 0x4000);
        assert_eq!(half(-1.0), 0);
    }

    #[test]
    fn rounded_corners_leave_out_less_further_in() {
        assert_eq!(inset(0, 0), 0);
        let cuts: Vec<u32> = (0..12).map(|y| inset(12, y)).collect();
        assert!(cuts.windows(2).all(|w| w[0] >= w[1]), "{cuts:?}");
        assert!(cuts[0] >= 6 && cuts[11] == 0, "{cuts:?}");
    }
}
