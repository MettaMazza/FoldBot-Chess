
/* Built-in: string concatenation */
long long concat(long long a, long long b) {
    const char* sa = (const char*)a;
    const char* sb = (const char*)b;
    long long la = strlen(sa);
    long long lb = strlen(sb);
    char* result = malloc(la + lb + 1);
    memcpy(result, sa, la);
    memcpy(result + la, sb, lb);
    result[la + lb] = '\0';
    ep_gc_register(result, EP_OBJ_STRING);
    return (long long)result;
}

long long int_to_string(long long val) {
    char* buf = malloc(32);
    snprintf(buf, 32, "%lld", val);
    ep_gc_register(buf, EP_OBJ_STRING);
    return (long long)buf;
}

long long ep_int_to_str(long long val) { return int_to_string(val); }

typedef struct { char* data; long long len; long long cap; } EpStringBuilder;

long long ep_sb_create(long long dummy) {
    (void)dummy;
    EpStringBuilder* sb = (EpStringBuilder*)malloc(sizeof(EpStringBuilder));
    sb->cap = 256;
    sb->len = 0;
    sb->data = (char*)malloc(sb->cap);
    sb->data[0] = '\0';
    return (long long)sb;
}

long long ep_sb_append(long long sb_ptr, long long str_ptr) {
    EpStringBuilder* sb = (EpStringBuilder*)sb_ptr;
    const char* s = (const char*)str_ptr;
    if (!s) return sb_ptr;
    long long slen = strlen(s);
    while (sb->len + slen + 1 > sb->cap) {
        sb->cap *= 2;
        sb->data = (char*)realloc(sb->data, sb->cap);
    }
    memcpy(sb->data + sb->len, s, slen);
    sb->len += slen;
    sb->data[sb->len] = '\0';
    return sb_ptr;
}

long long ep_sb_append_int(long long sb_ptr, long long val) {
    char buf[32];
    snprintf(buf, sizeof(buf), "%lld", val);
    return ep_sb_append(sb_ptr, (long long)buf);
}

long long ep_sb_to_string(long long sb_ptr) {
    EpStringBuilder* sb = (EpStringBuilder*)sb_ptr;
    char* result = (char*)malloc(sb->len + 1);
    memcpy(result, sb->data, sb->len + 1);
    ep_gc_register(result, EP_OBJ_STRING);
    free(sb->data);
    free(sb);
    return (long long)result;
}

long long ep_sb_length(long long sb_ptr) {
    return ((EpStringBuilder*)sb_ptr)->len;
}

long long str_to_ptr(long long s) { return s; }
long long ptr_to_str(long long p) {
    if (p == 0) return (long long)strdup("");
    char* copy = strdup((const char*)p);
    ep_gc_register(copy, EP_OBJ_STRING);
    return (long long)copy;
}

long long peek_byte(long long ptr, long long offset) {
    return (long long)((unsigned char*)ptr)[offset];
}
long long poke_byte(long long ptr, long long offset, long long value) {
    ((unsigned char*)ptr)[offset] = (unsigned char)value;
    return 0;
}
long long alloc_bytes(long long size) {
    return (long long)calloc((size_t)size, 1);
}
long long free_bytes(long long ptr) {
    free((void*)ptr);
    return 0;
}
long long list_to_bytes(long long list_ptr) {
    long long len = length_list(list_ptr);
    unsigned char* buf = (unsigned char*)malloc(len);
    for (long long i = 0; i < len; i++) {
        buf[i] = (unsigned char)get_list(list_ptr, i);
    }
    return (long long)buf;
}
long long bytes_to_list(long long ptr, long long len) {
    long long list = create_list();
    unsigned char* buf = (unsigned char*)ptr;
    for (long long i = 0; i < len; i++) {
        append_list(list, (long long)buf[i]);
    }
    return list;
}

long long ep_gc_get_minor_count() {
    return ep_gc_minor_count;
}
long long ep_gc_get_major_count() {
    return ep_gc_major_count;
}
long long ep_gc_get_nursery_count() {
    return ep_gc_nursery_count;
}

long long string_to_int(long long s) {
    if (s == 0) return 0;
    return atoll((const char*)s);
}

long long read_line() {
    char buf[4096];
    if (fgets(buf, sizeof(buf), stdin) == NULL) { buf[0] = '\0'; }
    size_t len = strlen(buf);
    if (len > 0 && buf[len-1] == '\n') buf[len-1] = '\0';
    char* result = strdup(buf);
    ep_gc_register(result, EP_OBJ_STRING);
    return (long long)result;
}

long long read_int() {
    long long val = 0;
    scanf("%lld", &val);
    while(getchar() != '\n');
    return val;
}

long long read_float() {
    double val = 0.0;
    scanf("%lf", &val);
    while(getchar() != '\n');
    long long result; memcpy(&result, &val, sizeof(double));
    return result;
}

long long int_to_float(long long val) {
    double d = (double)val;
    long long result; memcpy(&result, &d, sizeof(double));
    return result;
}

long long float_to_int(long long val) {
    double d; memcpy(&d, &val, sizeof(double));
    return (long long)d;
}

