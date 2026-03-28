use std::{
    env,
    fs::File,
    io::{self, BufReader, Write},
};

use byteorder::ReadBytesExt;

// ---------------------------------------------------------------------------
// Raw mode
// ---------------------------------------------------------------------------

struct RawModeGuard {
    saved: libc::termios,
}

impl RawModeGuard {
    fn enable() -> Self {
        let saved = unsafe {
            let mut t = std::mem::zeroed::<libc::termios>();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut t) != 0 {
                panic!("tcgetattr failed");
            }
            t
        };

        let mut raw = saved;
        unsafe {
            // Input: disable byte mangling and CR/NL translation so every
            // keystroke arrives as the terminal actually sent it.
            raw.c_iflag &= !(libc::IGNBRK | libc::BRKINT | libc::PARMRK
                | libc::ISTRIP | libc::INLCR | libc::IGNCR
                | libc::ICRNL  | libc::IXON);

            // Output: intentionally leave OPOST alone so \n is still
            // translated to \r\n — keeps our println! output readable.

            // Local: no echo, no canonical line buffering, no signal chars,
            // no extended processing.
            raw.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON
                | libc::ISIG | libc::IEXTEN);

            // Character width: 8-bit, no parity.
            raw.c_cflag &= !libc::PARENB;
            raw.c_cflag |= libc::CS8;

            // Block until at least 1 byte is available; no read timeout.
            raw.c_cc[libc::VMIN as usize]  = 1;
            raw.c_cc[libc::VTIME as usize] = 0;

            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &raw) != 0 {
                panic!("tcsetattr failed");
            }
        }

        RawModeGuard { saved }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &self.saved);
        }
    }
}

// ---------------------------------------------------------------------------
// Kitty Keyboard Protocol
// ---------------------------------------------------------------------------

// All five KKP flags:
//   1  disambiguate escape codes
//   2  report event types (press / repeat / release)
//   4  report alternate keys
//   8  report all keys as escape codes
//  16  report associated text
const KKP_FLAGS: u32 = 0b11111;

struct KkpGuard;

impl KkpGuard {
    fn enable() -> Self {
        print!("\x1b[>{}u", KKP_FLAGS);
        let _ = io::stdout().flush();
        KkpGuard
    }
}

impl Drop for KkpGuard {
    fn drop(&mut self) {
        print!("\x1b[<u");
        let _ = io::stdout().flush();
    }
}

// ---------------------------------------------------------------------------
// CSI-u decoding
// ---------------------------------------------------------------------------

// Decode a KKP unicode key code to a human-readable name.
// Printable Unicode codepoints are returned as the character itself;
// KKP-specific functional key codes (57xxx range) are named explicitly,
// sourced from the Kitty spec and verified against the alacritty source.
fn decode_key(code: u32) -> String {
    match code {
        8 | 127 => "Backspace".into(),
        9       => "Tab".into(),
        13      => "Enter".into(),
        27      => "Escape".into(),
        32      => "Space".into(),

        // KKP functional keys
        57358 => "CapsLock".into(),
        57359 => "ScrollLock".into(),
        57360 => "NumLock".into(),
        57361 => "PrintScreen".into(),
        57362 => "Pause".into(),
        57363 => "Menu".into(),

        // F13–F35
        57376 => "F13".into(),  57377 => "F14".into(),  57378 => "F15".into(),
        57379 => "F16".into(),  57380 => "F17".into(),  57381 => "F18".into(),
        57382 => "F19".into(),  57383 => "F20".into(),  57384 => "F21".into(),
        57385 => "F22".into(),  57386 => "F23".into(),  57387 => "F24".into(),
        57388 => "F25".into(),  57389 => "F26".into(),  57390 => "F27".into(),
        57391 => "F28".into(),  57392 => "F29".into(),  57393 => "F30".into(),
        57394 => "F31".into(),  57395 => "F32".into(),  57396 => "F33".into(),
        57397 => "F34".into(),  57398 => "F35".into(),

        // Numpad keys
        57399 => "Kp0".into(),    57400 => "Kp1".into(),    57401 => "Kp2".into(),
        57402 => "Kp3".into(),    57403 => "Kp4".into(),    57404 => "Kp5".into(),
        57405 => "Kp6".into(),    57406 => "Kp7".into(),    57407 => "Kp8".into(),
        57408 => "Kp9".into(),    57409 => "KpDecimal".into(),
        57410 => "KpDivide".into(), 57411 => "KpMultiply".into(),
        57412 => "KpMinus".into(),  57413 => "KpPlus".into(),
        57414 => "KpEnter".into(),  57415 => "KpEqual".into(),
        57416 => "KpSep".into(),
        57417 => "KpLeft".into(),   57418 => "KpRight".into(),
        57419 => "KpUp".into(),     57420 => "KpDown".into(),
        57421 => "KpPageUp".into(), 57422 => "KpPageDown".into(),
        57423 => "KpHome".into(),   57424 => "KpEnd".into(),
        57425 => "KpInsert".into(), 57426 => "KpDelete".into(),

        // Media / volume
        57428 => "MediaPlay".into(),       57429 => "MediaPause".into(),
        57430 => "MediaPlayPause".into(),  57432 => "MediaStop".into(),
        57433 => "MediaFastForward".into(),57434 => "MediaRewind".into(),
        57435 => "MediaNext".into(),       57436 => "MediaPrev".into(),
        57437 => "MediaRecord".into(),
        57438 => "VolDown".into(), 57439 => "VolUp".into(), 57440 => "Mute".into(),

        // Modifier keys (left / right)
        57441 => "LShift".into(),  57442 => "LCtrl".into(),
        57443 => "LAlt".into(),    57444 => "LSuper".into(),
        57445 => "LHyper".into(),  57446 => "LMeta".into(),
        57447 => "RShift".into(),  57448 => "RCtrl".into(),
        57449 => "RAlt".into(),    57450 => "RSuper".into(),
        57451 => "RHyper".into(),  57452 => "RMeta".into(),

        // Printable Unicode: just show the character
        c if (33..=126).contains(&c) || c > 127 => {
            char::from_u32(c)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| format!("U+{c:04X}"))
        }

        c => format!("U+{c:04X}"),
    }
}

// Decode the modifier value (mod_val) from a CSI-u sequence.
// KKP encodes modifiers as (bitmask + 1), so mod_val=1 means no modifiers.
// Bits: 0=Shift 1=Alt 2=Ctrl 3=Super 4=Hyper 5=Meta 6=CapsLock 7=NumLock
fn decode_modifiers(mod_val: u32) -> String {
    let bits = mod_val.saturating_sub(1);
    let names: &[(u32, &str)] = &[
        (0x01, "Shift"), (0x02, "Alt"),     (0x04, "Ctrl"),
        (0x08, "Super"), (0x10, "Hyper"),   (0x20, "Meta"),
        (0x40, "Caps"),  (0x80, "NumLock"),
    ];
    let parts: Vec<&str> = names.iter()
        .filter(|(bit, _)| bits & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    if parts.is_empty() { "none".into() } else { parts.join("+") }
}

// Parse and print a fully-read CSI-u parameter string (everything between
// the '[' and the 'u').
//
// Format (Kitty Keyboard Protocol):
//   key[:alt_key] [;mod_val[:event_type] [;text_codepoints]]
//
// Returns true if the caller should exit (Ctrl+D: keycode=100, Ctrl bit set).
fn print_csi_u(params: &[u8]) -> bool {
    let s = String::from_utf8_lossy(params);
    let mut sections = s.splitn(3, ';');

    let key_code: u32 = sections.next()
        .and_then(|p| p.split(':').next())
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);

    let mod_event  = sections.next().unwrap_or("1");
    let mut me     = mod_event.split(':');
    let mod_val: u32  = me.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let event_type: u32 = me.next().and_then(|s| s.parse().ok()).unwrap_or(1);

    // Ctrl+D in KKP mode: keycode 100 ('d'), Ctrl bit (0x04) set in modifier.
    let modifier_bits = mod_val.saturating_sub(1);
    if key_code == 100 && modifier_bits & 0x04 != 0 {
        return true;
    }

    let event_str = match event_type {
        2 => " repeat",
        3 => " release",
        _ => "",          // 1 = press, omit for brevity
    };

    println!(
        "KKP={} ({}){}, MOD={}",
        key_code,
        decode_key(key_code),
        event_str,
        decode_modifiers(mod_val),
    );

    false
}

// Handle ~-terminated CSI sequences.
// Returns true if the caller should exit (unused for now, kept symmetric
// with print_csi_u).
fn handle_csi_tilde(params: &[u8], paste_buf: &mut Option<Vec<u8>>) -> bool {
    match params {
        b"200" => { *paste_buf = Some(Vec::new()); println!("PASTE BEGIN"); }
        b"201" => {
            if let Some(buf) = paste_buf.take() {
                println!("PASTE TEXT: {:?}", String::from_utf8_lossy(&buf));
            }
            println!("PASTE END");
        }
        other => println!("    ESC [ {} ~    ", String::from_utf8_lossy(other)),
    }
    false
}

// ---------------------------------------------------------------------------
// Bracketed paste
// ---------------------------------------------------------------------------

// Request bracketed paste mode from the terminal (\x1b[?2004h).
// Without this, the terminal sends pasted text as raw keystrokes and never
// emits the ESC[200~ / ESC[201~ markers we rely on.
struct BracketedPasteGuard;

impl BracketedPasteGuard {
    fn enable() -> Self {
        print!("\x1b[?2004h");
        let _ = io::stdout().flush();
        BracketedPasteGuard
    }
}

impl Drop for BracketedPasteGuard {
    fn drop(&mut self) {
        print!("\x1b[?2004l");
        let _ = io::stdout().flush();
    }
}

// ---------------------------------------------------------------------------
// Input loops
// ---------------------------------------------------------------------------

// Read CSI parameter bytes up to and including the final byte (0x40–0x7E).
// Returns None on EOF/error.
fn read_csi_params(br: &mut BufReader<File>) -> Option<(Vec<u8>, u8)> {
    let mut params = Vec::new();
    loop {
        match br.read_u8() {
            Ok(b) if (0x40..=0x7E).contains(&b) => return Some((params, b)),
            Ok(b)  => params.push(b),
            Err(_) => return None,
        }
    }
}

// Dispatch a fully-read CSI sequence.  When inside a bracketed paste all
// sequences except the ESC[201~ end marker are accumulated as raw bytes.
// Returns true if the caller should exit.
fn dispatch_csi(params: Vec<u8>, term: u8, paste_buf: &mut Option<Vec<u8>>) -> bool {
    // Inside a paste: accumulate everything except the end marker.
    if paste_buf.is_some() && !(term == b'~' && params == b"201") {
        let buf = paste_buf.as_mut().unwrap();
        buf.push(0x1B); buf.push(b'[');
        buf.extend_from_slice(&params);
        buf.push(term);
        return false;
    }
    match term {
        b'u' => print_csi_u(&params),
        b'~' => handle_csi_tilde(&params, paste_buf),
        _    => { println!("    ESC [ {} {}    ", String::from_utf8_lossy(&params), term as char); false }
    }
}

// Process one byte in KKP mode.  Returns true if the caller should exit.
fn process_kkp_byte(b: u8, csi: &mut bool, paste_buf: &mut Option<Vec<u8>>, br: &mut BufReader<File>) -> bool {
    if b == 0x1B {
        *csi = true;
        return false;
    }

    if !*csi {
        // Plain byte outside an escape sequence — only valid inside a paste.
        match paste_buf {
            Some(buf) => { buf.push(b); return false; }
            None      => { println!("error: unexpected byte 0x{b:02x}"); return true; }
        }
    }

    *csi = false;

    if b != b'[' {
        // ESC followed by something other than '['.
        match paste_buf {
            Some(buf) => { buf.push(0x1B); buf.push(b); }
            None      => println!("unexpected: ESC followed by 0x{b:02x}"),
        }
        return false;
    }

    let Some((params, term)) = read_csi_params(br) else { return true; };
    dispatch_csi(params, term, paste_buf)
}

fn run_legacy(br: &mut BufReader<File>) {
    loop {
        match br.read_u8() {
            Ok(0x04) | Err(_) => break,
            Ok(b) => println!("    legacy={:02X}    ", b),
        }
    }
}

fn run_kkp(br: &mut BufReader<File>) {
    let mut csi = false;
    let mut paste_buf: Option<Vec<u8>> = None;
    loop {
        let b = match br.read_u8() {
            Ok(0x04) | Err(_) => break,
            Ok(b) => b,
        };
        if process_kkp_byte(b, &mut csi, &mut paste_buf, br) { break; }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = env::args().collect();
    let kkp = args.iter().any(|a| a == "--kkp");

    let _raw   = RawModeGuard::enable();
    let _paste = BracketedPasteGuard::enable();
    let _kkp   = if kkp { Some(KkpGuard::enable()) } else { None };

    let mut br = BufReader::new(File::open("/dev/stdin").unwrap());
    if kkp { run_kkp(&mut br) } else { run_legacy(&mut br) }
}
