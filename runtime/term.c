/* term.c — Implementation of minimal ncurses-like terminal library.
 * Uses ANSI escape codes + termios for raw mode.
 * No external dependencies beyond libc. */

#define _POSIX_C_SOURCE 200809L
#define _DEFAULT_SOURCE

#include "term.h"

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/select.h>
#include <sys/time.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

/* ---- global state ------------------------------------------------------- */

static struct termios g_orig_termios;
static int g_initialized = 0;
static int g_raw_mode = 0;
static int g_mouse_enabled = 0;
static int g_cursor_hidden = 0;

/* ---- helpers ------------------------------------------------------------ */

static void term_die(const char *msg) {
    if (g_raw_mode) {
        tcsetattr(STDIN_FILENO, TCSAFLUSH, &g_orig_termios);
        printf("\x1b[?1049l\x1b[?25h"); /* exit alt screen, show cursor */
        fflush(stdout);
    }
    fprintf(stderr, "term: %s: %s\n", msg, strerror(errno));
    exit(1);
}

static void term_restore(void) {
    if (g_raw_mode) {
        tcsetattr(STDIN_FILENO, TCSAFLUSH, &g_orig_termios);
        g_raw_mode = 0;
    }
    if (g_cursor_hidden) {
        printf("\x1b[?25h");
        g_cursor_hidden = 0;
    }
    if (g_mouse_enabled) {
        printf("\x1b[?1000l\x1b[?1002l\x1b[?1003l");
        g_mouse_enabled = 0;
    }
    /* exit alternate screen buffer */
    printf("\x1b[?1049l");
    fflush(stdout);
}

static void term_sigint(int sig) {
    (void)sig;
    term_restore();
    _exit(130);
}

/* ---- initialization ----------------------------------------------------- */

int term_init(void) {
    if (g_initialized) return 0;

    /* save original terminal settings */
    if (tcgetattr(STDIN_FILENO, &g_orig_termios) != 0) {
        return -1;
    }

    struct termios raw = g_orig_termios;
    /* input modes: no break, no CR->NL, no parity, no strip */
    raw.c_iflag &= ~(BRKINT | ICRNL | INPCK | ISTRIP | IXON);
    /* output modes: disable post-processing */
    raw.c_oflag &= ~(OPOST);
    /* control modes: 8-bit chars */
    raw.c_cflag |= (CS8);
    /* local modes: no echo, no canonical, no signals, no extended */
    raw.c_lflag &= ~(ECHO | ICANON | ISIG | IEXTEN);
    /* control chars: read returns immediately with min 0, timeout 0 */
    raw.c_cc[VMIN] = 0;
    raw.c_cc[VTIME] = 0;

    if (tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw) != 0) {
        return -1;
    }
    g_raw_mode = 1;

    /* enter alternate screen buffer, hide cursor, enable mouse */
    printf("\x1b[?1049h\x1b[?25l");
    fflush(stdout);
    g_cursor_hidden = 1;

    /* set up signal handlers for cleanup */
    signal(SIGINT, term_sigint);
    signal(SIGTERM, term_sigint);
    signal(SIGHUP, term_sigint);

    atexit(term_restore);

    g_initialized = 1;
    return 0;
}

void term_cleanup(void) {
    term_restore();
    g_initialized = 0;
}

/* ---- screen ------------------------------------------------------------- */

void term_clear(void) {
    printf("\x1b[2J\x1b[H");
    fflush(stdout);
}

void term_clear_to_end(void) {
    printf("\x1b[J");
    fflush(stdout);
}

void term_move(int row, int col) {
    /* ANSI uses 1-indexed */
    printf("\x1b[%d;%dH", row + 1, col + 1);
    fflush(stdout);
}

void term_size(int *rows, int *cols) {
    struct winsize ws;
    if (ioctl(STDOUT_FILENO, TIOCGWINSZ, &ws) == 0) {
        *rows = ws.ws_row;
        *cols = ws.ws_col;
    } else {
        *rows = 24;
        *cols = 80;
    }
}

/* ---- output ------------------------------------------------------------- */

void term_print(const char *s) {
    fputs(s, stdout);
    fflush(stdout);
}

void term_printf(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    vprintf(fmt, ap);
    va_end(ap);
    fflush(stdout);
}

void term_putc(char c) {
    putchar(c);
    fflush(stdout);
}

/* ---- colors ------------------------------------------------------------- */

void term_fg(TermColor c) {
    int code = (c <= 7) ? 30 + c : 90 + (c - 8);
    printf("\x1b[%dm", code);
    fflush(stdout);
}

void term_bg(TermColor c) {
    int code = (c <= 7) ? 40 + c : 100 + (c - 8);
    printf("\x1b[%dm", code);
    fflush(stdout);
}

void term_reset_colors(void) {
    printf("\x1b[0m");
    fflush(stdout);
}

/* ---- input -------------------------------------------------------------- */

int term_kbhit(void) {
    struct pollfd pfd = { .fd = STDIN_FILENO, .events = POLLIN };
    return poll(&pfd, 1, 0) > 0;
}

static int term_read_escape_sequence(char *buf, int max_len, int timeout_ms) {
    int len = 0;
    struct pollfd pfd = { .fd = STDIN_FILENO, .events = POLLIN };

    while (len < max_len - 1) {
        int ret = poll(&pfd, 1, timeout_ms);
        if (ret <= 0) break;
        char c;
        if (read(STDIN_FILENO, &c, 1) != 1) break;
        buf[len++] = c;
        if (c >= 'A' && c <= '~') break; /* typical end of escape sequence */
    }
    buf[len] = '\0';
    return len;
}

int term_getch(void) {
    return term_getch_timeout(-1);
}

int term_getch_timeout(int ms) {
    struct pollfd pfd = { .fd = STDIN_FILENO, .events = POLLIN };
    int timeout = (ms < 0) ? -1 : ms;

    if (poll(&pfd, 1, timeout) <= 0) {
        return 0; /* timeout */
    }

    char c;
    if (read(STDIN_FILENO, &c, 1) != 1) {
        return -1;
    }

    if (c == '\x1b') {
        /* Escape sequence - read more */
        char seq[16];
        int len = term_read_escape_sequence(seq, sizeof(seq), 50);

        if (len == 0) {
            return TERM_KEY_ESC;
        }

        /* CSI sequences: ESC [ ... */
        if (seq[0] == '[') {
            if (len == 2) {
                switch (seq[1]) {
                    case 'A': return TERM_KEY_UP;
                    case 'B': return TERM_KEY_DOWN;
                    case 'C': return TERM_KEY_RIGHT;
                    case 'D': return TERM_KEY_LEFT;
                    case 'H': return TERM_KEY_HOME;
                    case 'F': return TERM_KEY_END;
                    case 'Z': return TERM_KEY_TAB; /* Shift+Tab */
                }
            } else if (len >= 3 && seq[len - 1] == '~') {
                /* ESC [ N ~ */
                char num_str[8];
                int n = 0;
                for (int i = 1; i < len - 1 && n < 7; i++) {
                    if (seq[i] >= '0' && seq[i] <= '9') {
                        num_str[n++] = seq[i];
                    }
                }
                num_str[n] = '\0';
                if (n > 0) {
                    int code = atoi(num_str);
                    switch (code) {
                        case 1: return TERM_KEY_HOME;
                        case 2: return TERM_KEY_INS;
                        case 3: return TERM_KEY_DEL;
                        case 4: return TERM_KEY_END;
                        case 5: return TERM_KEY_PGUP;
                        case 6: return TERM_KEY_PGDN;
                        case 11: return TERM_KEY_F(1);
                        case 12: return TERM_KEY_F(2);
                        case 13: return TERM_KEY_F(3);
                        case 14: return TERM_KEY_F(4);
                        case 15: return TERM_KEY_F(5);
                        case 17: return TERM_KEY_F(6);
                        case 18: return TERM_KEY_F(7);
                        case 19: return TERM_KEY_F(8);
                        case 20: return TERM_KEY_F(9);
                        case 21: return TERM_KEY_F(10);
                        case 23: return TERM_KEY_F(11);
                        case 24: return TERM_KEY_F(12);
                    }
                }
            } else if (len >= 4 && seq[1] == '1' && seq[2] == ';') {
                /* ESC [ 1 ; N [A-D] - modified keys (shift/ctrl/alt) */
                /* For now, ignore modifiers and treat as base key */
                if (seq[3] >= 'A' && seq[3] <= 'D') {
                    switch (seq[3]) {
                        case 'A': return TERM_KEY_UP;
                        case 'B': return TERM_KEY_DOWN;
                        case 'C': return TERM_KEY_RIGHT;
                        case 'D': return TERM_KEY_LEFT;
                    }
                }
            }
        } else if (seq[0] == 'O') {
            /* SS3 sequences: ESC O ... (some function keys) */
            if (len == 2) {
                switch (seq[1]) {
                    case 'P': return TERM_KEY_F(1);
                    case 'Q': return TERM_KEY_F(2);
                    case 'R': return TERM_KEY_F(3);
                    case 'S': return TERM_KEY_F(4);
                }
            }
        }

        /* Unknown escape sequence - return ESC */
        return TERM_KEY_ESC;
    }

    if (c == '\r' || c == '\n') return TERM_KEY_ENTER;
    if (c == '\t') return TERM_KEY_TAB;
    if (c == 127 || c == '\b') return TERM_KEY_BACKSPACE;

    return (unsigned char)c;
}

/* ---- timing ------------------------------------------------------------- */

void term_sleep_ms(int ms) {
    struct timespec ts;
    ts.tv_sec = ms / 1000;
    ts.tv_nsec = (ms % 1000) * 1000000L;
    nanosleep(&ts, NULL);
}

uint64_t term_now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000 + (uint64_t)ts.tv_nsec / 1000000;
}

/* ---- cursor ------------------------------------------------------------- */

void term_hide_cursor(void) {
    if (!g_cursor_hidden) {
        printf("\x1b[?25l");
        fflush(stdout);
        g_cursor_hidden = 1;
    }
}

void term_show_cursor(void) {
    if (g_cursor_hidden) {
        printf("\x1b[?25h");
        fflush(stdout);
        g_cursor_hidden = 0;
    }
}

/* ---- mouse -------------------------------------------------------------- */

void term_mouse_on(void) {
    if (!g_mouse_enabled) {
        /* Enable mouse tracking (SGR protocol = 1006) */
        printf("\x1b[?1000h\x1b[?1006h"); /* VT200 + SGR */
        fflush(stdout);
        g_mouse_enabled = 1;
    }
}

void term_mouse_off(void) {
    if (g_mouse_enabled) {
        printf("\x1b[?1000l\x1b[?1006l");
        fflush(stdout);
        g_mouse_enabled = 0;
    }
}

int term_mouse_event(int *row, int *col, int *btn) {
    if (!g_mouse_enabled) return 0;
    if (!term_kbhit()) return 0;

    /* SGR mouse protocol: ESC [ < btn ; col ; row M/m */
    char c;
    if (read(STDIN_FILENO, &c, 1) != 1 || c != '\x1b') return 0;
    if (read(STDIN_FILENO, &c, 1) != 1 || c != '[') return 0;
    if (read(STDIN_FILENO, &c, 1) != 1 || c != '<') return 0;

    /* parse button */
    int btn_code = 0;
    while (read(STDIN_FILENO, &c, 1) == 1) {
        if (c >= '0' && c <= '9') {
            btn_code = btn_code * 10 + (c - '0');
        } else if (c == ';') {
            break;
        } else {
            return 0;
        }
    }

    /* parse col */
    int col_code = 0;
    while (read(STDIN_FILENO, &c, 1) == 1) {
        if (c >= '0' && c <= '9') {
            col_code = col_code * 10 + (c - '0');
        } else if (c == ';') {
            break;
        } else {
            return 0;
        }
    }

    /* parse row */
    int row_code = 0;
    while (read(STDIN_FILENO, &c, 1) == 1) {
        if (c >= '0' && c <= '9') {
            row_code = row_code * 10 + (c - '0');
        } else if (c == 'M' || c == 'm') {
            break;
        } else {
            return 0;
        }
    }

    /* button: 0=press left, 1=press middle, 2=press right, 3=release, 64+=wheel */
    *btn = (btn_code & 3) + 1; /* 1=left, 2=middle, 3=right */
    if (c == 'm') *btn = 0; /* release */

    /* SGR protocol uses 1-indexed */
    *col = col_code - 1;
    *row = row_code - 1;

    return 1;
}