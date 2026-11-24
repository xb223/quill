// Real-time terminal Pong, using the bundled `term` library.
// Play against an AI opponent. Build and run with:
//     quillc run examples/pong.q

#add <term.h>

extern fn term_init() -> int;
extern fn term_cleanup() -> void;
extern fn term_clear() -> void;
extern fn term_move(row: int, col: int) -> void;
extern fn term_print(s: str) -> void;
extern fn term_fg(color: int) -> void;
extern fn term_reset_colors() -> void;
extern fn term_getch_timeout(ms: int) -> int;
extern fn term_now_ms() -> int;
extern fn term_sleep_ms(ms: int) -> void;
extern fn term_hide_cursor() -> void;

fn clamp(v: int, lo: int, hi: int) -> int {
    if v < lo { lo } else if v > hi { hi } else { v }
}

fn draw_cell(row: int, col: int, ch: str, color: int) {
    term_move(row, col);
    term_fg(color);
    term_print(ch);
}

fn draw_border(w: int, h: int) {
    let mut col = 0;
    while col < w {
        draw_cell(0, col, "-", 4);
        draw_cell(h - 1, col, "-", 4);
        col += 1;
    }
    let mut row = 1;
    while row < h - 1 {
        draw_cell(row, 0, "|", 4);
        draw_cell(row, w - 1, "|", 4);
        row += 1;
    }
    let mut r = 1;
    while r < h - 1 {
        draw_cell(r, w / 2, ":", 8);
        r += 2;
    }
}

fn main() {
    term_init();
    term_hide_cursor();
    term_clear();

    let W = 40;
    let H = 18;
    let mut left_y = H / 2 - 2;
    let mut right_y = H / 2 - 2;
    let mut bx = W / 2;
    let mut by = H / 2;
    let mut vx = 1;
    let mut vy = 1;
    let mut left_score = 0;
    let mut right_score = 0;
    let mut running = true;

    while running {
        let frame_start = term_now_ms();

        // ---- input (non-blocking) ----
        let key = term_getch_timeout(0);
        if key == 113 || key == 27 {        // 'q' or Esc
            running = false;
        } else if key == 119 {              // 'w'
            left_y = clamp(left_y - 1, 0, H - 4);
        } else if key == 115 {              // 's'
            left_y = clamp(left_y + 1, 0, H - 4);
        }

        // ---- AI: tracks the ball lazily ----
        let center = right_y + 2;
        if by < center - 1 {
            right_y = clamp(right_y - 1, 0, H - 4);
        } else if by > center + 1 {
            right_y = clamp(right_y + 1, 0, H - 4);
        }

        // ---- ball physics ----
        bx = bx + vx;
        by = by + vy;
        if by <= 1 { by = 1; vy = -vy; }
        if by >= H - 2 { by = H - 2; vy = -vy; }

        if bx <= 1 {
            if by >= left_y && by < left_y + 4 {
                bx = 2;
                vx = -vx;
            } else {
                right_score += 1;
                bx = W / 2; by = H / 2; vx = 1; vy = 1;
            }
        }
        if bx >= W - 2 {
            if by >= right_y && by < right_y + 4 {
                bx = W - 3;
                vx = -vx;
            } else {
                left_score += 1;
                bx = W / 2; by = H / 2; vx = -1; vy = 1;
            }
        }

        if left_score >= 5 || right_score >= 5 {
            running = false;
        }

        // ---- draw ----
        term_clear();
        draw_border(W, H);

        let mut i = 0;
        while i < 4 {
            draw_cell(left_y + i, 1, "#", 2);
            draw_cell(right_y + i, W - 2, "#", 3);
            i += 1;
        }
        draw_cell(by, bx, "o", 1);

        term_reset_colors();
        term_move(H, 0);
        term_print("  " + str(left_score) + " : " + str(right_score) + "   [w/s] move   [q] quit");

        // ---- fixed ~60 fps ----
        let elapsed = term_now_ms() - frame_start;
        if elapsed < 16 {
            term_sleep_ms(16 - elapsed);
        }
    }

    term_clear();
    term_reset_colors();
    term_move(1, 1);
    if left_score > right_score {
        term_print("You win! " + str(left_score) + " : " + str(right_score));
    } else if right_score > left_score {
        term_print("AI wins! " + str(left_score) + " : " + str(right_score));
    } else {
        term_print("Tie! " + str(left_score) + " : " + str(right_score));
    }
    term_print("   (press any key to exit)");
    term_getch_timeout(3000);
    term_cleanup();
}
