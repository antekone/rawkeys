# The what

A terminal key inspection tool that shows you exactly what bytes your
terminal sends for each key press -- in either raw hex or decoded
[Kitty Keyboard Protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)
(CSI u) form.

Built while debugging a tmux 3.5a paste-garbling bug. The full story
is in the blog post
[Living among the dinosaurs in the TUI world](https://anadoxin.org/blog/terminal-multiplexers/).

## The why

Terminal key encoding is a layered mess. A key press travels through
your GUI toolkit → terminal emulator → optional SSH hop(s) → optional
terminal multiplexer(s) → the application. Any layer can silently
transform the bytes. When something breaks, you need a way to see exactly
what bytes land at the end of the chain.

`rawkeys` puts the terminal in raw mode, optionally enables the Kitty
Keyboard Protocol and bracketed paste, and prints what it receives so
you can verify every hop is behaving.

## The how

Requires a Rust toolchain (stable).

```bash
cargo build --release
# binary at: target/release/rawkeys
```

## The notes

### Legacy mode (plain hex dump)

```
rawkeys
```

Puts stdin in raw mode and prints every byte as `legacy=XX`. Useful
for inspecting what your terminal or multiplexer sends without any
additional decoding layer. Exit with **Ctrl+D**.

Example -- pressing **Enter** on a raw terminal:

```
legacy=0D
```

### KKP mode (Kitty Keyboard Protocol)

```
rawkeys --kkp
```

In addition to raw mode, this:

1. Enables the Kitty Keyboard Protocol (`ESC[>31u`) so the terminal
   starts sending structured `ESC[codepoint;modifier u` sequences
   instead of legacy bytes.
2. Enables bracketed paste (`ESC[?2004h`) so pasted text is wrapped in
   `ESC[200~` … `ESC[201~` markers rather than arriving as a stream of
   raw keystrokes.

All three are cleanly disabled on exit (even on Ctrl+C, via RAII
guards), so your shell is left in a consistent state.

Exit with **Ctrl+D**.

#### Key events

Each key prints one line:

```
KKP=<codepoint> (<name>) [<event>], MOD=<modifiers>
```

The `[event]` field is omitted for plain key presses; it shows
`repeat` or `release` when those event types are reported.

Example output for a few keystrokes:

```
KKP=106 (j), MOD=none
KKP=13 (Enter), MOD=none
KKP=13 (Enter), MOD=Shift
KKP=106 (j), MOD=Ctrl
KKP=57441 (LShift), MOD=Shift
KKP=57441 (LShift) release, MOD=none
KKP=9 (Tab), MOD=Shift
```

#### Bracketed paste

```
PASTE BEGIN
PASTE TEXT: "hello\nworld"
PASTE END
```

The pasted text is shown as a Rust debug string, so non-printable
bytes (newlines, escapes, …) are visible as `\n`, `\x1b`, etc.

#### Non-KKP sequences

Sequences that use a terminator other than `u` (e.g. F1–F4 which end
in `P`/`Q`/`R`/`S`) are printed raw:

```
    ESC [ 1 ; 1 P
```

## Running directly in the terminal emulator

For accurate results, **run rawkeys directly in your terminal emulator
without a multiplexer in the way.** If you run it inside tmux or
screen, all I/O goes through the multiplexer's own PTY layer, which
re-encodes key events according to its own rules before they reach
rawkeys. That is a different (and also interesting) thing to measure,
but it is not the same as measuring what the terminal emulator itself
produces.

A quick check:

```bash
echo $TMUX        # should be empty
echo $TERM        # should name your terminal, e.g. alacritty
```

## Shell reset after a crash

If rawkeys is killed with `SIGKILL` the RAII guards cannot run, and
the terminal may be left in KKP mode. Best would be to run the tool
and chain a `reset` program immediately after it, like:

    ./target/debug/rawkeys --kkp ; reset

This will ensure that the shell runs `reset` after rawkeys exit, even
if `rawkeys` will receive a `SIGKILL`.
