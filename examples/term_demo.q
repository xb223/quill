// Showcase of the bundled `term` library: cursor movement, colors, timing
// and non-blocking input. Run with: quillc run examples/term_demo.q

#add <term.h>

extern fn term_init() -> int;
extern fn term_cleanup() -> void;
extern fn term_clear() -> void;
extern fn term_move(row: int, col: int) -> void;
extern fn term_print(s: str) -> void;
extern fn term_fg(color: int) -> void;
extern fn term_reset_colors() -> void;
extern fn term_hide_cursor() -> void;
extern fn term_sleep_ms(ms: int) -> void;
extern fn term_getch_timeout(ms: int) -> int;

fn line(row: int, text: str, color: int) {
    term_move(row, 2);
    term_fg(color);
    term_print(text);
}

fn main() {
    term_init();
    term_hide_cursor();
    term_clear();

    line(1, "  Quill terminal library  ", 6);
    line(3, "cursor movement", 2);
    line(4, "16 ANSI colors", 3);
    line(5, "non-blocking key input", 4);
    line(6, "millisecond timing", 5);
    line(8, "Animating a counter, press any key to stop...", 7);

    let mut n = 0;
    let mut running = true;
    while running {
        term_move(10, 2);
        term_fg(6);
        term_print("count: " + str(n));
        n += 1;

        // Non-blocking: returns 0 when no key is waiting.
        if term_getch_timeout(0) != 0 {
            running = false;
        }
        term_sleep_ms(50);
    }

    term_reset_colors();
    term_move(12, 2);
    term_print("done. goodbye!");
    term_sleep_ms(600);
    term_cleanup();
}
