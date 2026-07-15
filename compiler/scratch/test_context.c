#define _XOPEN_SOURCE 600
#include <stdio.h>
#include <stdlib.h>
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
#include <ucontext.h>

ucontext_t main_ctx, fiber_ctx;
char fiber_stack[64 * 1024];

void fiber_func(void) {
    printf("Inside fiber\n");
    swapcontext(&fiber_ctx, &main_ctx);
    printf("Inside fiber again\n");
    swapcontext(&fiber_ctx, &main_ctx);
}

int main() {
    getcontext(&fiber_ctx);
    fiber_ctx.uc_stack.ss_sp = fiber_stack;
    fiber_ctx.uc_stack.ss_size = sizeof(fiber_stack);
    fiber_ctx.uc_link = &main_ctx;
    makecontext(&fiber_ctx, fiber_func, 0);

    printf("Swapping to fiber\n");
    swapcontext(&main_ctx, &fiber_ctx);
    printf("Back in main\n");
    swapcontext(&main_ctx, &fiber_ctx);
    printf("End of main\n");
    return 0;
}
