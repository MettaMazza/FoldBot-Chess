#include <stdio.h>
#include <stdlib.h>

typedef struct {
    long long x;
    long long y;
    long long z;
} Record;

int main(void) {
    int n = 100000;
    Record** records = (Record**)malloc(n * sizeof(Record*));
    for (int i = 0; i < n; i++) {
        records[i] = (Record*)malloc(sizeof(Record));
        records[i]->x = i;
        records[i]->y = i * 2;
        records[i]->z = i * 3;
    }
    long long total = 0;
    for (int j = 0; j < n; j++) {
        total += records[j]->z;
    }
    printf("Sum of z field: %lld\n", total);
    for (int i = 0; i < n; i++) {
        free(records[i]);
    }
    free(records);
    return 0;
}
