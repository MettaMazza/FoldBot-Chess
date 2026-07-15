#include <stdio.h>

int state = 0;
int i = 0;

int step() {
    switch (state) {
        case 0:
            for (i = 0; i < 3; i++) {
                state = 1;
                return 42; // Yield
                case 1:
                printf("Resumed loop at i = %d\n", i);
            }
            state = 2;
            return 99;
        case 2:
            printf("Finished\n");
            return -1;
    }
    return -1;
}

int main() {
    printf("Step 1: %d\n", step());
    printf("Step 2: %d\n", step());
    printf("Step 3: %d\n", step());
    printf("Step 4: %d\n", step());
    return 0;
}
