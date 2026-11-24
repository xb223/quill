/* Quill runtime: strings, arrays, checked arithmetic and a process-lifetime
 * arena allocator. Kept deliberately small and readable; the generated program
 * only ever touches the functions declared in quill_rt.h. */

#define _POSIX_C_SOURCE 200809L

#include "quill_rt.h"

#include <errno.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>

/* ---- arena ------------------------------------------------------------- */

typedef struct QChunk {
    struct QChunk* next;
    size_t used;
    size_t cap;
    char data[];
} QChunk;

static QChunk* q_chunks = NULL;

void* q_alloc(int64_t size) {
    if (size < 0) {
        q_panic("out of memory: negative allocation size");
    }
    size_t need = ((size_t)size + 15u) & ~(size_t)15u;
    if (need == 0) {
        need = 16;
    }
    if (q_chunks == NULL || q_chunks->used + need > q_chunks->cap) {
        size_t cap = need > 65536u ? need : 65536u;
        QChunk* chunk = (QChunk*)malloc(sizeof(QChunk) + cap);
        if (chunk == NULL) {
            q_panic("out of memory");
        }
        chunk->next = q_chunks;
        chunk->used = 0;
        chunk->cap = cap;
        q_chunks = chunk;
    }
    void* p = q_chunks->data + q_chunks->used;
    q_chunks->used += need;
    return p;
}

void q_panic(const char* message) {
    fflush(stdout);
    fprintf(stderr, "panic: %s\n", message);
    exit(101);
}

void q_unreachable(const char* message) {
    q_panic(message);
}

void q_assert(bool cond, const char* message) {
    if (!cond) {
        q_panic(message);
    }
}

/* ---- strings ----------------------------------------------------------- */

static QStr* q_str_copy(const char* data, int64_t len) {
    char* buffer = (char*)q_alloc(len + 1);
    if (len > 0) {
        memcpy(buffer, data, (size_t)len);
    }
    buffer[len] = '\0';
    QStr* s = (QStr*)q_alloc((int64_t)sizeof(QStr));
    s->len = len;
    s->data = buffer;
    return s;
}

QStr* q_str_lit(const char* data, int64_t len) {
    QStr* s = (QStr*)q_alloc((int64_t)sizeof(QStr));
    s->len = len;
    s->data = data;
    return s;
}

QStr* q_str_concat(const QStr* a, const QStr* b) {
    int64_t len = a->len + b->len;
    char* buffer = (char*)q_alloc(len + 1);
    if (a->len > 0) {
        memcpy(buffer, a->data, (size_t)a->len);
    }
    if (b->len > 0) {
        memcpy(buffer + a->len, b->data, (size_t)b->len);
    }
    buffer[len] = '\0';
    return q_str_lit(buffer, len);
}

int64_t q_str_len(const QStr* s) {
    return s->len;
}

int64_t q_str_cmp(const QStr* a, const QStr* b) {
    int64_t n = a->len < b->len ? a->len : b->len;
    int cmp = n > 0 ? memcmp(a->data, b->data, (size_t)n) : 0;
    if (cmp != 0) {
        return cmp < 0 ? -1 : 1;
    }
    if (a->len == b->len) {
        return 0;
    }
    return a->len < b->len ? -1 : 1;
}

bool q_str_eq(const QStr* a, const QStr* b) {
    return a->len == b->len && (a->len == 0 || memcmp(a->data, b->data, (size_t)a->len) == 0);
}

/* Shortest representation that round-trips, matching what people expect:
 * 1.0 prints as "1", 0.1 as "0.1", 1e9 as "1000000000". */
static void q_format_f64(char* buffer, size_t cap, double v) {
    if (isnan(v)) {
        snprintf(buffer, cap, "nan");
        return;
    }
    if (isinf(v)) {
        snprintf(buffer, cap, v < 0 ? "-inf" : "inf");
        return;
    }
    if (v == 0.0) {
        snprintf(buffer, cap, "0");
        return;
    }
    if (v == floor(v) && fabs(v) < 1e21) {
        snprintf(buffer, cap, "%.0f", v);
        return;
    }
    for (int precision = 1; precision <= 17; precision++) {
        snprintf(buffer, cap, "%.*g", precision, v);
        if (strtod(buffer, NULL) == v) {
            return;
        }
    }
}

QStr* q_str_of_i64(int64_t v) {
    char buffer[32];
    int n = snprintf(buffer, sizeof buffer, "%lld", (long long)v);
    return q_str_copy(buffer, (int64_t)n);
}

QStr* q_str_of_f64(double v) {
    char buffer[64];
    q_format_f64(buffer, sizeof buffer, v);
    return q_str_copy(buffer, (int64_t)strlen(buffer));
}

QStr* q_str_of_bool(bool v) {
    return q_str_lit(v ? "true" : "false", v ? 4 : 5);
}

char* q_str_cstr(const QStr* s) {
    char* buffer = (char*)q_alloc(s->len + 1);
    if (s->len > 0) {
        memcpy(buffer, s->data, (size_t)s->len);
    }
    buffer[s->len] = '\0';
    return buffer;
}

static char* q_str_cstring(const QStr* s) {
    return q_str_cstr(s);
}

int64_t q_str_to_i64(const QStr* s) {
    char* text = q_str_cstring(s);
    errno = 0;
    char* end = NULL;
    long long value = strtoll(text, &end, 10);
    while (end != NULL && (*end == ' ' || *end == '\t' || *end == '\n' || *end == '\r')) {
        end++;
    }
    if (errno == ERANGE) {
        q_panic("cannot convert string to int: value is out of range");
    }
    if (end == text || end == NULL || *end != '\0') {
        char message[160];
        snprintf(message, sizeof message, "cannot convert string \"%.100s\" to int", text);
        q_panic(message);
    }
    return (int64_t)value;
}

double q_str_to_f64(const QStr* s) {
    char* text = q_str_cstring(s);
    errno = 0;
    char* end = NULL;
    double value = strtod(text, &end);
    while (end != NULL && (*end == ' ' || *end == '\t' || *end == '\n' || *end == '\r')) {
        end++;
    }
    if (errno == ERANGE) {
        q_panic("cannot convert string to float: value is out of range");
    }
    if (end == text || end == NULL || *end != '\0') {
        char message[160];
        snprintf(message, sizeof message, "cannot convert string \"%.100s\" to float", text);
        q_panic(message);
    }
    return value;
}

/* ---- input ------------------------------------------------------------- */

QStr* q_read_line(void) {
    char* line = NULL;
    size_t cap = 0;
    ssize_t n = getline(&line, &cap, stdin);
    if (n < 0) {
        free(line);
        return q_str_lit("", 0);
    }
    while (n > 0 && (line[n - 1] == '\n' || line[n - 1] == '\r')) {
        n--;
    }
    char* copy = (char*)q_alloc(n + 1);
    if (n > 0) {
        memcpy(copy, line, (size_t)n);
    }
    copy[n] = '\0';
    QStr* s = (QStr*)q_alloc((int64_t)sizeof(QStr));
    s->len = n;
    s->data = copy;
    free(line);
    return s;
}

int64_t q_read_int(void) {
    return q_str_to_i64(q_read_line());
}

double q_read_float(void) {
    return q_str_to_f64(q_read_line());
}

/* ---- output ------------------------------------------------------------ */

void q_print_i64(int64_t v) {
    printf("%lld", (long long)v);
}

void q_print_f64(double v) {
    char buffer[64];
    q_format_f64(buffer, sizeof buffer, v);
    fputs(buffer, stdout);
}

void q_print_bool(bool v) {
    fputs(v ? "true" : "false", stdout);
}

void q_print_str(const QStr* s) {
    if (s->len > 0) {
        fwrite(s->data, 1, (size_t)s->len, stdout);
    }
}

void q_print_newline(void) {
    fputc('\n', stdout);
}

void q_print_arr_i64(const QArr* a) {
    fputc('[', stdout);
    for (int64_t i = 0; i < a->len; i++) {
        if (i > 0) {
            fputs(", ", stdout);
        }
        q_print_i64(*(const int64_t*)(a->data + i * a->elem_size));
    }
    fputc(']', stdout);
}

void q_print_arr_f64(const QArr* a) {
    fputc('[', stdout);
    for (int64_t i = 0; i < a->len; i++) {
        if (i > 0) {
            fputs(", ", stdout);
        }
        q_print_f64(*(const double*)(a->data + i * a->elem_size));
    }
    fputc(']', stdout);
}

void q_print_arr_bool(const QArr* a) {
    fputc('[', stdout);
    for (int64_t i = 0; i < a->len; i++) {
        if (i > 0) {
            fputs(", ", stdout);
        }
        q_print_bool(*(const bool*)(a->data + i * a->elem_size));
    }
    fputc(']', stdout);
}

void q_print_arr_str(const QArr* a) {
    fputc('[', stdout);
    for (int64_t i = 0; i < a->len; i++) {
        if (i > 0) {
            fputs(", ", stdout);
        }
        q_print_str(*(QStr**)(a->data + i * a->elem_size));
    }
    fputc(']', stdout);
}

/* ---- checked integer arithmetic ---------------------------------------- */

int64_t q_add_i64(int64_t a, int64_t b) {
    int64_t result;
    if (__builtin_add_overflow(a, b, &result)) {
        q_panic("integer overflow in `+`");
    }
    return result;
}

int64_t q_sub_i64(int64_t a, int64_t b) {
    int64_t result;
    if (__builtin_sub_overflow(a, b, &result)) {
        q_panic("integer overflow in `-`");
    }
    return result;
}

int64_t q_mul_i64(int64_t a, int64_t b) {
    int64_t result;
    if (__builtin_mul_overflow(a, b, &result)) {
        q_panic("integer overflow in `*`");
    }
    return result;
}

int64_t q_div_i64(int64_t a, int64_t b) {
    if (b == 0) {
        q_panic("division by zero");
    }
    if (a == INT64_MIN && b == -1) {
        q_panic("integer overflow in `/`");
    }
    return a / b;
}

int64_t q_mod_i64(int64_t a, int64_t b) {
    if (b == 0) {
        q_panic("division by zero in `%`");
    }
    if (a == INT64_MIN && b == -1) {
        q_panic("integer overflow in `%`");
    }
    return a % b;
}

int64_t q_neg_i64(int64_t a) {
    if (a == INT64_MIN) {
        q_panic("integer overflow in unary `-`");
    }
    return -a;
}

int64_t q_abs_i64(int64_t a) {
    if (a == INT64_MIN) {
        q_panic("integer overflow in `abs`");
    }
    return a < 0 ? -a : a;
}

int64_t q_min_i64(int64_t a, int64_t b) {
    return a < b ? a : b;
}

int64_t q_max_i64(int64_t a, int64_t b) {
    return a > b ? a : b;
}

double q_min_f64(double a, double b) {
    return a < b ? a : b;
}

double q_max_f64(double a, double b) {
    return a > b ? a : b;
}

int64_t q_f64_to_i64(double v) {
    if (isnan(v)) {
        q_panic("cannot convert NaN to int");
    }
    if (v < -9223372036854775808.0 || v >= 9223372036854775808.0) {
        q_panic("float value is out of range for int");
    }
    return (int64_t)v;
}

/* ---- arrays ------------------------------------------------------------ */

QArr* q_arr_new(int64_t elem_size, int64_t cap) {
    if (cap < 4) {
        cap = 4;
    }
    QArr* a = (QArr*)q_alloc((int64_t)sizeof(QArr));
    a->len = 0;
    a->cap = cap;
    a->elem_size = elem_size;
    a->data = (char*)q_alloc(elem_size * cap);
    return a;
}

QArr* q_arr_from(const void* data, int64_t len, int64_t elem_size) {
    QArr* a = q_arr_new(elem_size, len < 4 ? 4 : len);
    if (len > 0) {
        memcpy(a->data, data, (size_t)(len * elem_size));
    }
    a->len = len;
    return a;
}

void q_arr_push(QArr* a, const void* value) {
    if (a->len == a->cap) {
        int64_t next_cap = a->cap == 0 ? 4 : a->cap * 2;
        char* next = (char*)q_alloc(next_cap * a->elem_size);
        if (a->len > 0) {
            memcpy(next, a->data, (size_t)(a->len * a->elem_size));
        }
        a->data = next;
        a->cap = next_cap;
    }
    memcpy(a->data + a->len * a->elem_size, value, (size_t)a->elem_size);
    a->len++;
}

void* q_arr_at(const QArr* a, int64_t index) {
    if (index < 0 || index >= a->len) {
        char message[128];
        snprintf(message, sizeof message,
                 "array index %lld is out of bounds (length %lld)",
                 (long long)index, (long long)a->len);
        q_panic(message);
    }
    return a->data + index * a->elem_size;
}

int64_t q_arr_len(const QArr* a) {
    return a->len;
}
