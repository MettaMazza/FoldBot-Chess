#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(void) {
    int limit = 1000000;
    char* sieve = (char*)malloc(limit + 1);
    memset(sieve, 1, limit + 1);
    sieve[0] = 0;
    sieve[1] = 0;
    for (int p = 2; p * p <= limit; p++) {
        if (sieve[p]) {
            for (int j = p * p; j <= limit; j += p) {
                sieve[j] = 0;
            }
        }
    }
    int count = 0;
    for (int k = 2; k <= limit; k++) {
        if (sieve[k]) count++;
    }
    printf("Primes up to %d: %d\n", limit, count);
    free(sieve);
    return 0;
}
