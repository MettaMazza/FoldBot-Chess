#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(void) {
    // Build a string by concatenating 100,000 integers
    long long cap = 1024;
    long long len = 0;
    char* result = (char*)malloc(cap);
    result[0] = '\0';

    char buf[32];
    for (int i = 0; i < 100000; i++) {
        int n = snprintf(buf, sizeof(buf), "%d", i);
        while (len + n + 1 > cap) {
            cap *= 2;
            result = (char*)realloc(result, cap);
        }
        memcpy(result + len, buf, n);
        len += n;
        result[len] = '\0';
    }

    printf("String length: %lld\n", len);
    free(result);
    return 0;
}
