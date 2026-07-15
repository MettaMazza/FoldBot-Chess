#include <stdio.h>

int state = 0;
int step() {
    switch (state) {
        case 0: {
            int val = 10 + ({
                state = 1;
                return 42;
                case 1:
                5;
            });
            printf("val = %d\n", val);
            break;
        }
    }
    return 99;
}

int main() {
    step();
    step();
    return 0;
}
