/* term.h — Minimal ncurses-like terminal library for Quill.
 * Provides: non-blocking input, cursor positioning, screen control, colors.
 * Uses ANSI escape codes + termios for raw mode. */

#ifndef QUILL_TERM_H
#define QUILL_TERM_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ---- initialization ----------------------------------------------------- */

// Initialize terminal: raw mode, hide cursor, clear screen, enable mouse.
// Returns 0 on success, -1 on failure.
int term_init(void);

// Cleanup: restore terminal, show cursor.
void term_cleanup(void);

/* ---- screen ------------------------------------------------------------- */

// Clear screen and move cursor to (0,0).
void term_clear(void);

// Clear from cursor to end of screen.
void term_clear_to_end(void);

// Move cursor to (row, col) — 0-indexed.
void term_move(int row, int col);

// Get terminal size.
void term_size(int *rows, int *cols);

/* ---- output ------------------------------------------------------------- */

// Print string at current cursor position.
void term_print(const char *s);

// Print formatted string at current cursor position.
void term_printf(const char *fmt, ...);

// Print a single character.
void term_putc(char c);

/* ---- colors ------------------------------------------------------------- */

// Color indices (0-7 standard, 8-15 bright)
typedef enum {
    TERM_BLACK = 0,
    TERM_RED = 1,
    TERM_GREEN = 2,
    TERM_YELLOW = 3,
    TERM_BLUE = 4,
    TERM_MAGENTA = 5,
    TERM_CYAN = 6,
    TERM_WHITE = 7,
    TERM_BRIGHT_BLACK = 8,
    TERM_BRIGHT_RED = 9,
    TERM_BRIGHT_GREEN = 10,
    TERM_BRIGHT_YELLOW = 11,
    TERM_BRIGHT_BLUE = 12,
    TERM_BRIGHT_MAGENTA = 13,
    TERM_BRIGHT_CYAN = 14,
    TERM_BRIGHT_WHITE = 15,
} TermColor;

// Set foreground color.
void term_fg(TermColor c);

// Set background color.
void term_bg(TermColor c);

// Reset colors to default.
void term_reset_colors(void);

/* ---- input -------------------------------------------------------------- */

// Check if a key is available (non-blocking). Returns 1 if yes, 0 if no.
int term_kbhit(void);

// Read a key (blocking). Returns key code or -1 on error.
// Special keys return negative codes (see below).
int term_getch(void);

// Read a key with timeout in milliseconds.
// Returns key code, 0 on timeout, -1 on error.
int term_getch_timeout(int ms);

/* Special key codes (negative) */
#define TERM_KEY_UP       -1001
#define TERM_KEY_DOWN     -1002
#define TERM_KEY_LEFT     -1003
#define TERM_KEY_RIGHT    -1004
#define TERM_KEY_ENTER    -1005
#define TERM_KEY_ESC      -1006
#define TERM_KEY_BACKSPACE -1007
#define TERM_KEY_TAB      -1008
#define TERM_KEY_HOME     -1009
#define TERM_KEY_END      -1010
#define TERM_KEY_PGUP     -1011
#define TERM_KEY_PGDN     -1012
#define TERM_KEY_INS      -1013
#define TERM_KEY_DEL      -1014
#define TERM_KEY_F(n)     (-1100 - (n))  /* F1-F12: -1101 to -1112 */

/* ---- timing ------------------------------------------------------------- */

// Sleep for milliseconds.
void term_sleep_ms(int ms);

// Get current time in milliseconds (monotonic).
uint64_t term_now_ms(void);

/* ---- cursor ------------------------------------------------------------- */

// Hide cursor.
void term_hide_cursor(void);

// Show cursor.
void term_show_cursor(void);

/* ---- mouse -------------------------------------------------------------- */

// Enable mouse tracking (requires term_init).
void term_mouse_on(void);

// Disable mouse tracking.
void term_mouse_off(void);

// Check for mouse event. Fills *row, *col, *btn (1=left, 2=middle, 3=right).
// Returns 1 if event available, 0 otherwise.
int term_mouse_event(int *row, int *col, int *btn);

#ifdef __cplusplus
}
#endif

#endif /* QUILL_TERM_H */