#ifndef QUILL_RT_H
#define QUILL_RT_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

/* Immutable string: a length and a byte pointer. Not NUL-terminated by
 * contract; data may point at a static literal or arena memory. */
typedef struct QStr {
    int64_t len;
    const char* data;
} QStr;

/* Growable, bounds-checked array. Element size is fixed at creation. */
typedef struct QArr {
    int64_t len;
    int64_t cap;
    int64_t elem_size;
    char* data;
} QArr;

/* ---- memory ------------------------------------------------------------ */

void* q_alloc(int64_t size);
_Noreturn void q_panic(const char* message);
_Noreturn void q_unreachable(const char* message);
void q_assert(bool cond, const char* message);

/* ---- strings ----------------------------------------------------------- */

QStr* q_str_lit(const char* data, int64_t len);
QStr* q_str_concat(const QStr* a, const QStr* b);
QStr* q_str_of_i64(int64_t v);
QStr* q_str_of_f64(double v);
QStr* q_str_of_bool(bool v);
int64_t q_str_len(const QStr* s);
int64_t q_str_cmp(const QStr* a, const QStr* b);
bool q_str_eq(const QStr* a, const QStr* b);
int64_t q_str_to_i64(const QStr* s);
double q_str_to_f64(const QStr* s);

/* ---- input ------------------------------------------------------------- */

QStr* q_read_line(void);
int64_t q_read_int(void);
double q_read_float(void);

/* ---- output ------------------------------------------------------------ */

void q_print_i64(int64_t v);
void q_print_f64(double v);
void q_print_bool(bool v);
void q_print_str(const QStr* s);
void q_print_newline(void);
void q_print_arr_i64(const QArr* a);
void q_print_arr_f64(const QArr* a);
void q_print_arr_bool(const QArr* a);
void q_print_arr_str(const QArr* a);

/* ---- checked integer arithmetic ---------------------------------------- */

int64_t q_add_i64(int64_t a, int64_t b);
int64_t q_sub_i64(int64_t a, int64_t b);
int64_t q_mul_i64(int64_t a, int64_t b);
int64_t q_div_i64(int64_t a, int64_t b);
int64_t q_mod_i64(int64_t a, int64_t b);
int64_t q_neg_i64(int64_t a);
int64_t q_abs_i64(int64_t a);
int64_t q_min_i64(int64_t a, int64_t b);
int64_t q_max_i64(int64_t a, int64_t b);
double q_min_f64(double a, double b);
double q_max_f64(double a, double b);
int64_t q_f64_to_i64(double v);

/* ---- arrays ------------------------------------------------------------ */

QArr* q_arr_new(int64_t elem_size, int64_t cap);
QArr* q_arr_from(const void* data, int64_t len, int64_t elem_size);
void q_arr_push(QArr* a, const void* value);
void* q_arr_at(const QArr* a, int64_t index);
int64_t q_arr_len(const QArr* a);

#endif /* QUILL_RT_H */
