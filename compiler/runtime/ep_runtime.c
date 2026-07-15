#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#ifdef __wasm__
#define _SETJMP_H
typedef int jmp_buf[1];
#define setjmp(buf) (0)
#define longjmp(buf, val) abort()

// Mock pthreads for single-threaded WASM
typedef struct { int lock_state; } pthread_mutex_t;
typedef struct { int cond_state; } pthread_cond_t;
typedef struct { int rw_state; } pthread_rwlock_t;
typedef int pthread_t;
typedef int pthread_attr_t;
#define PTHREAD_MUTEX_INITIALIZER {0}
#define PTHREAD_COND_INITIALIZER {0}
#define PTHREAD_RWLOCK_INITIALIZER {0}
#define pthread_mutex_init(m, a) ((void)(a), (m)->lock_state = 0, 0)
#define pthread_mutex_lock(m) ((m)->lock_state = 1, 0)
#define pthread_mutex_unlock(m) ((m)->lock_state = 0, 0)
#define pthread_mutex_trylock(m) ((m)->lock_state == 0 ? ((m)->lock_state = 1, 0) : 1)
#define pthread_mutex_destroy(m) ((void)(m), 0)
#define pthread_cond_init(c, a) ((void)(a), (c)->cond_state = 0, 0)
#define pthread_cond_wait(c, m) ((void)(c), (void)(m), 0)
#define pthread_cond_signal(c) ((void)(c), 0)
#define pthread_cond_broadcast(c) ((void)(c), 0)
#define pthread_cond_destroy(c) ((void)(c), 0)
#define pthread_rwlock_init(r, a) ((void)(a), (r)->rw_state = 0, 0)
#define pthread_rwlock_rdlock(r) ((r)->rw_state = 1, 0)
#define pthread_rwlock_wrlock(r) ((r)->rw_state = 2, 0)
#define pthread_rwlock_unlock(r) ((r)->rw_state = 0, 0)
#define pthread_rwlock_destroy(r) ((void)(r), 0)
#define pthread_create(t, a, f, arg) ((void)(t), (void)(a), (void)(f), (void)(arg), 0)
#define pthread_join(t, r) ((void)(t), (void)(r), 0)
#define pthread_detach(t) ((void)(t), 0)
#else
#include <setjmp.h>
#endif
#include <signal.h>
#include <time.h>
#ifndef _WIN32
#include <unistd.h>
#endif
#if defined(__APPLE__)
#include <mach/mach.h>
#endif
#if defined(__linux__)
#include <sys/random.h>
#endif
#include <fcntl.h>

/* Cryptographically secure random bytes. Uses the OS CSPRNG: arc4random on
   Apple/BSD, getrandom(2) on Linux (falling back to /dev/urandom), and a
   /dev/urandom read elsewhere. Only if all of those are unavailable does it
   fall back to rand() — never on a supported platform. */
static void ep_secure_random_bytes(unsigned char* buf, size_t n) {
#if defined(__APPLE__) || defined(__FreeBSD__) || defined(__OpenBSD__) || defined(__NetBSD__)
    arc4random_buf(buf, n);
#else
    size_t got = 0;
  #if defined(__linux__)
    while (got < n) {
        ssize_t r = getrandom(buf + got, n - got, 0);
        if (r <= 0) break;
        got += (size_t)r;
    }
  #endif
    if (got < n) {
        FILE* f = fopen("/dev/urandom", "rb");
        if (f) {
            got += fread(buf + got, 1, n - got, f);
            fclose(f);
        }
    }
    while (got < n) {
        buf[got++] = (unsigned char)(rand() & 0xFF);
    }
#endif
}

/* Try/catch infrastructure */
static jmp_buf ep_try_buf;
static volatile int ep_try_active = 0;

static void ep_signal_handler(int sig) {
    if (ep_try_active) {
        ep_try_active = 0;
        longjmp(ep_try_buf, sig);
    }
    /* Outside try: print error and exit */
    const char* name = sig == SIGSEGV ? "segmentation fault (null pointer or invalid memory access)"
                     : sig == SIGFPE  ? "arithmetic error (division by zero)"
                     : sig == SIGABRT ? "aborted"
                     : "unknown signal";
    fprintf(stderr, "\nRuntime Error: %s (signal %d)\n", name, sig);

    /* Write to daemon/general log file if environment variable is set */
    const char* daemon_log = getenv("ERNOS_DAEMON_LOG");
    if (!daemon_log || daemon_log[0] == '\0') {
        daemon_log = getenv("ERNOS_LOG_FILE");
    }
    if (daemon_log && daemon_log[0] != '\0') {
        FILE* f = fopen(daemon_log, "ab");
        if (f) {
            time_t rawtime;
            time(&rawtime);
            struct tm * timeinfo = localtime(&rawtime);
            char time_buf[80];
            if (timeinfo) {
                strftime(time_buf, sizeof(time_buf), "%Y-%m-%d %H:%M:%S", timeinfo);
            } else {
                snprintf(time_buf, sizeof(time_buf), "%lld", (long long)rawtime);
            }
            fprintf(f, "[%s] FATAL: Runtime Error: %s (signal %d)\n", time_buf, name, sig);
            fclose(f);
        }
    }

    _exit(128 + sig);
}

#ifdef _MSC_VER
static void ep_install_signal_handlers(void);
#pragma section(".CRT$XCU", read)
__declspec(allocate(".CRT$XCU")) static void (*_ep_init_signals)(void) = ep_install_signal_handlers;
static void ep_install_signal_handlers(void) {
#else
__attribute__((constructor))
static void ep_install_signal_handlers(void) {
#endif
    signal(SIGFPE, ep_signal_handler);
    signal(SIGSEGV, ep_signal_handler);
    signal(SIGABRT, ep_signal_handler);
#ifdef _WIN32
    { WSADATA wsa; WSAStartup(MAKEWORD(2,2), &wsa); }
#endif
}

#if defined(__wasm__)
  typedef int ep_thread_t;
  typedef int ep_mutex_t;
  typedef int ep_cond_t;
  #define ep_mutex_init(m) (void)(0)
  #define ep_mutex_lock(m) (void)(0)
  #define ep_mutex_unlock(m) (void)(0)
  #define ep_cond_init(c) (void)(0)
  #define ep_cond_wait(c, m) (void)(0)
  #define ep_cond_signal(c) (void)(0)
#elif defined(_WIN32)
  #include <winsock2.h>
  #include <ws2tcpip.h>
  #include <windows.h>
  #pragma comment(lib, "ws2_32.lib")
  typedef HANDLE ep_thread_t;
  typedef CRITICAL_SECTION ep_mutex_t;
  typedef CONDITION_VARIABLE ep_cond_t;
  #define ep_mutex_init(m) InitializeCriticalSection(m)
  #define ep_mutex_lock(m) EnterCriticalSection(m)
  #define ep_mutex_unlock(m) LeaveCriticalSection(m)
  #define ep_cond_init(c) InitializeConditionVariable(c)
  #define ep_cond_wait(c, m) SleepConditionVariableCS(c, m, INFINITE)
  #define ep_cond_signal(c) WakeConditionVariable(c)
#else
  #include <sys/socket.h>
  #include <netinet/in.h>
  #include <arpa/inet.h>
  #include <unistd.h>
  #include <netdb.h>
  #include <fcntl.h>
  #include <errno.h>
  #include <sys/select.h>
  #include <pthread.h>
  typedef pthread_t ep_thread_t;
  typedef pthread_mutex_t ep_mutex_t;
  typedef pthread_cond_t ep_cond_t;
  #define ep_mutex_init(m) pthread_mutex_init(m, NULL)
  #define ep_mutex_lock(m) pthread_mutex_lock(m)
  #define ep_mutex_unlock(m) pthread_mutex_unlock(m)
  #define ep_cond_init(c) pthread_cond_init(c, NULL)
  #define ep_cond_wait(c, m) pthread_cond_wait(c, m)
  #define ep_cond_signal(c) pthread_cond_signal(c)
#endif

/* ========== Ernos Mark-and-Sweep Garbage Collector ========== */

#include <setjmp.h>
#if !defined(__wasm__) && !defined(_WIN32)
#include <pthread.h>
#endif

typedef enum {
    EP_OBJ_LIST,
    EP_OBJ_STRING,
    EP_OBJ_STRUCT,
    EP_OBJ_CLOSURE,
    EP_OBJ_MAP
} EpObjKind;

typedef struct EpGCObject {
    EpObjKind kind;
    int marked;
    void* ptr;                /* actual allocation pointer */
    long long size;           /* payload size for structs */
    long long num_fields;     /* number of fields for structs (each is long long) */
    int generation;           /* 0 = Nursery/young, 1 = Old */
    struct EpGCObject* next;  /* intrusive linked list */
} EpGCObject;

long long ep_time_now_ms(void);
long long ep_sleep_ms(long long ms);

typedef struct EpTask EpTask;
typedef struct {
    long long chan;
    int completed;
    long long value;
    EpTask* waiting_task;
} EpFuture;

static long long ep_await_future(EpFuture* fut);

struct EpTask {
    long long (*step)(void*); /* pointer to step function */
    void* args;               /* pointer to step state arguments */
    long long args_size_bytes; /* size of args struct for GC tracing */
    EpTask* next;             /* run-queue link pointer */
    EpFuture* fut;            /* future associated with this task */
    int state;                /* coroutine execution state */
    int is_cancelled;         /* cancellation flag for structured concurrency */
    struct EpTask* parent;    /* parent task for structured concurrency cancellation */
};

/* Event Loop Scheduler Globals & Functions */
static EpTask* ep_run_queue_head = NULL;
static EpTask* ep_run_queue_tail = NULL;
static EpTask* ep_current_task = NULL;
static int ep_event_loop_fd = -1; /* epoll or kqueue fd */
static int ep_active_io_sources = 0;

static void ep_task_enqueue(EpTask* task) {
    if (!task) return;
    task->next = NULL;
    if (ep_run_queue_tail) {
        ep_run_queue_tail->next = task;
        ep_run_queue_tail = task;
    } else {
        ep_run_queue_head = ep_run_queue_tail = task;
    }
}

static EpTask* ep_task_dequeue(void) {
    if (!ep_run_queue_head) return NULL;
    EpTask* task = ep_run_queue_head;
    ep_run_queue_head = ep_run_queue_head->next;
    if (!ep_run_queue_head) ep_run_queue_tail = NULL;
    return task;
}

#ifndef __wasm__
#ifdef __APPLE__
#include <sys/event.h>
#else
#include <sys/epoll.h>
#endif
#endif

static void ep_async_loop_init(void) {
    if (ep_event_loop_fd != -1) return;
#ifdef __wasm__
    ep_event_loop_fd = 999;
#elif defined(__APPLE__)
    ep_event_loop_fd = kqueue();
#else
    ep_event_loop_fd = epoll_create1(0);
#endif
}

typedef struct EpTimer {
    long long expiry_ms;
    EpTask* task;
    struct EpTimer* next;
} EpTimer;
static EpTimer* ep_timers_head = NULL;

static void ep_async_register_timer(long long timeout_ms, EpTask* task) {
    long long expiry = ep_time_now_ms() + timeout_ms;
    EpTimer* timer = (EpTimer*)malloc(sizeof(EpTimer));
    timer->expiry_ms = expiry;
    timer->task = task;
    timer->next = NULL;

    /* Insert sorted */
    if (!ep_timers_head || expiry < ep_timers_head->expiry_ms) {
        timer->next = ep_timers_head;
        ep_timers_head = timer;
    } else {
        EpTimer* cur = ep_timers_head;
        while (cur->next && cur->next->expiry_ms <= expiry) {
            cur = cur->next;
        }
        timer->next = cur->next;
        cur->next = timer;
    }
}

static long long ep_get_next_timer_timeout(void) {
    if (!ep_timers_head) return -1; /* block indefinitely */
    long long now = ep_time_now_ms();
    long long diff = ep_timers_head->expiry_ms - now;
    return diff < 0 ? 0 : diff;
}

static void ep_process_expired_timers(void) {
    long long now = ep_time_now_ms();
    while (ep_timers_head && ep_timers_head->expiry_ms <= now) {
        EpTimer* expired = ep_timers_head;
        ep_timers_head = ep_timers_head->next;
        ep_task_enqueue(expired->task);
        free(expired);
    }
}

static void ep_async_register_read(int fd, EpTask* task) {
#ifdef __wasm__
    (void)fd;
    (void)task;
#else
    ep_async_loop_init();
    ep_active_io_sources++;
#ifdef __APPLE__
    struct kevent ev;
    EV_SET(&ev, fd, EVFILT_READ, EV_ADD | EV_ONESHOT, 0, 0, task);
    kevent(ep_event_loop_fd, &ev, 1, NULL, 0, NULL);
#else
    struct epoll_event ev;
    ev.events = EPOLLIN | EPOLLONESHOT;
    ev.data.ptr = task;
    if (epoll_ctl(ep_event_loop_fd, EPOLL_CTL_ADD, fd, &ev) < 0) {
        epoll_ctl(ep_event_loop_fd, EPOLL_CTL_MOD, fd, &ev);
    }
#endif
#endif
}

static void ep_async_wait_step(long long timeout) {
#ifdef __wasm__
    if (timeout > 0) {
        ep_sleep_ms(timeout);
    }
#else
#ifdef __APPLE__
    struct kevent events[16];
    struct timespec ts;
    struct timespec* p_ts = NULL;
    if (timeout >= 0) {
        ts.tv_sec = timeout / 1000;
        ts.tv_nsec = (timeout % 1000) * 1000000;
        p_ts = &ts;
    }
    int n = kevent(ep_event_loop_fd, NULL, 0, events, 16, p_ts);
    for (int i = 0; i < n; i++) {
        EpTask* t = (EpTask*)events[i].udata;
        ep_task_enqueue(t);
        ep_active_io_sources--;
    }
#else
    struct epoll_event events[16];
    int n = epoll_wait(ep_event_loop_fd, events, 16, (int)timeout);
    for (int i = 0; i < n; i++) {
        EpTask* t = (EpTask*)events[i].data.ptr;
        ep_task_enqueue(t);
        ep_active_io_sources--;
    }
#endif
#endif
    ep_process_expired_timers();
}

static void ep_async_loop_run(void) {
    ep_async_loop_init();
    while (ep_run_queue_head || ep_timers_head || ep_active_io_sources > 0) {
        /* 1. Run all runnable tasks */
        while (ep_run_queue_head) {
            EpTask* task = ep_task_dequeue();
            if (task->is_cancelled) {
                if (task->fut) {
                    task->fut->completed = 1;
                    task->fut->value = -1;
                }
                free(task->args);
                free(task);
                continue;
            }
            ep_current_task = task;
            long long res = task->step(task->args);
            ep_current_task = NULL;
            if (res != -999999) {
                if (task->fut) {
                    task->fut->value = res;
                    task->fut->completed = 1;
                    if (task->fut->waiting_task) {
                        ep_task_enqueue(task->fut->waiting_task);
                        task->fut->waiting_task = NULL;
                    }
                }
                free(task->args);
                free(task);
            }
        }

        /* 2. If no tasks runnable, wait for I/O / timers */
        if (!ep_run_queue_head) {
            long long timeout = ep_get_next_timer_timeout();
            if (timeout == -1 && !ep_timers_head && ep_active_io_sources == 0) {
                break;
            }

            if (ep_event_loop_fd == -1) {
                if (timeout > 0) {
                    ep_sleep_ms(timeout);
                }
                ep_process_expired_timers();
                continue;
            }

            ep_async_wait_step(timeout);
        }
    }
}

static long long ep_await_future(EpFuture* fut) {
    if (!fut) return 0;
    while (!fut->completed) {
        if (ep_run_queue_head) {
            EpTask* task = ep_task_dequeue();
            if (task) {
                if (task->is_cancelled) {
                    if (task->fut) {
                        task->fut->completed = 1;
                        task->fut->value = -1;
                    }
                    free(task->args);
                    free(task);
                } else {
                    EpTask* saved_current = ep_current_task;
                    ep_current_task = task;
                    long long res = task->step(task->args);
                    ep_current_task = saved_current;
                    if (res != -999999) {
                        if (task->fut) {
                            task->fut->value = res;
                            task->fut->completed = 1;
                            if (task->fut->waiting_task) {
                                ep_task_enqueue(task->fut->waiting_task);
                                task->fut->waiting_task = NULL;
                            }
                        }
                        free(task->args);
                        free(task);
                    }
                }
            }
        } else {
            long long timeout = ep_get_next_timer_timeout();
            if (timeout == -1 && !ep_timers_head && ep_active_io_sources == 0) {
                fprintf(stderr, "Deadlock detected: awaiting incomplete future with no active tasks or timers.\n");
                exit(1);
            }
            if (ep_event_loop_fd == -1) {
                if (timeout > 0) {
                    ep_sleep_ms(timeout);
                }
                ep_process_expired_timers();
            } else {
                ep_async_wait_step(timeout);
            }
        }
    }
    return fut->value;
}

static EpGCObject* ep_gc_register(void* ptr, EpObjKind kind);
long long create_list(void);
long long append_list(long long list_ptr, long long value);

typedef struct {
    EpFuture* futures[128];
    int count;
    int has_error;
} EpTaskGroup;

typedef struct {
    EpFuture* fut;
    int timer_fired;
} EpTimeoutArgs;

static EpTask* ep_find_task_by_future(EpFuture* fut) {
    if (!fut) return NULL;
    EpTask* cur = ep_run_queue_head;
    while (cur) {
        if (cur->fut == fut) return cur;
        cur = cur->next;
    }
    EpTimer* timer = ep_timers_head;
    while (timer) {
        if (timer->task && timer->task->fut == fut) return timer->task;
        timer = timer->next;
    }
    return NULL;
}

static void ep_cancel_task(EpTask* task) {
    if (!task) return;
    task->is_cancelled = 1;
    if (task->fut) {
        task->fut->completed = 1;
        task->fut->value = -1;
    }
    // Cancel children in run queue
    EpTask* cur = ep_run_queue_head;
    while (cur) {
        if (cur->parent == task) {
            ep_cancel_task(cur);
        }
        cur = cur->next;
    }
    // Cancel children in timers queue
    EpTimer* timer = ep_timers_head;
    while (timer) {
        if (timer->task && timer->task->parent == task) {
            ep_cancel_task(timer->task);
        }
        timer = timer->next;
    }
}

static long long create_task_group(void) {
    EpTaskGroup* tg = (EpTaskGroup*)calloc(1, sizeof(EpTaskGroup));
    tg->count = 0;
    tg->has_error = 0;
    { EpGCObject* _go = ep_gc_register(tg, EP_OBJ_STRUCT); if(_go) _go->num_fields = 0; }
    return (long long)tg;
}

static long long add_task_group(long long group_ptr, long long fut_ptr) {
    EpTaskGroup* tg = (EpTaskGroup*)group_ptr;
    EpFuture* fut = (EpFuture*)fut_ptr;
    if (!tg || !fut) return 0;
    if (tg->count < 128) {
        tg->futures[tg->count++] = fut;
        // Associate the task's parent with the current task so it's cancellation-linked
        EpTask* task = ep_find_task_by_future(fut);
        if (task) {
            task->parent = ep_current_task;
        }
    }
    return 0;
}

static long long wait_task_group(long long group_ptr) {
    EpTaskGroup* tg = (EpTaskGroup*)group_ptr;
    if (!tg) return 0;
    
    int all_done = 0;
    while (!all_done) {
        all_done = 1;
        for (int i = 0; i < tg->count; i++) {
            EpFuture* fut = tg->futures[i];
            if (!fut->completed) {
                all_done = 0;
                break;
            }
        }
        
        if (all_done) break;
        
        if (ep_run_queue_head) {
            EpTask* task = ep_task_dequeue();
            if (task) {
                if (task->is_cancelled) {
                    if (task->fut) {
                        task->fut->completed = 1;
                        task->fut->value = -1;
                    }
                    free(task->args);
                    free(task);
                } else {
                    EpTask* saved_current = ep_current_task;
                    ep_current_task = task;
                    long long res = task->step(task->args);
                    ep_current_task = saved_current;
                    if (res != -999999) {
                        if (task->fut) {
                            task->fut->value = res;
                            task->fut->completed = 1;
                            if (task->fut->waiting_task) {
                                ep_task_enqueue(task->fut->waiting_task);
                                task->fut->waiting_task = NULL;
                            }
                        }
                        free(task->args);
                        free(task);
                    }
                }
            }
        } else {
            long long timeout = ep_get_next_timer_timeout();
            if (timeout == -1 && !ep_timers_head && ep_active_io_sources == 0) {
                fprintf(stderr, "Deadlock detected: waiting on task group with no active tasks or timers.\n");
                exit(1);
            }
            if (ep_event_loop_fd == -1) {
                if (timeout > 0) {
                    ep_sleep_ms(timeout);
                }
                ep_process_expired_timers();
            } else {
                ep_async_wait_step(timeout);
            }
        }
        
        // Propagate cancellation/failure inside task group
        for (int i = 0; i < tg->count; i++) {
            EpFuture* fut = tg->futures[i];
            if (fut->completed && fut->value == -1) {
                tg->has_error = 1;
                for (int j = 0; j < tg->count; j++) {
                    EpFuture* other_fut = tg->futures[j];
                    if (!other_fut->completed) {
                        EpTask* other_task = ep_find_task_by_future(other_fut);
                        if (other_task) {
                            ep_cancel_task(other_task);
                        } else {
                            other_fut->completed = 1;
                            other_fut->value = -1;
                        }
                    }
                }
            }
        }
    }
    
    long long list = create_list();
    for (int i = 0; i < tg->count; i++) {
        append_list(list, tg->futures[i]->value);
    }
    return list;
}

static long long ep_timeout_timer_step(void* r) {
    EpTimeoutArgs* args = (EpTimeoutArgs*)r;
    if (args && args->fut && !args->fut->completed) {
        args->timer_fired = 1;
        EpTask* task = ep_find_task_by_future(args->fut);
        if (task) {
            ep_cancel_task(task);
        } else {
            args->fut->completed = 1;
            args->fut->value = -1;
        }
    }
    return 0;
}

static long long async_timeout(long long timeout_ms, long long fut_ptr) {
    EpFuture* fut = (EpFuture*)fut_ptr;
    if (!fut) return -1;
    if (fut->completed) return fut->value;
    
    EpTimeoutArgs* args = (EpTimeoutArgs*)malloc(sizeof(EpTimeoutArgs));
    args->fut = fut;
    args->timer_fired = 0;
    
    EpTask* timer_task = (EpTask*)malloc(sizeof(EpTask));
    timer_task->step = ep_timeout_timer_step;
    timer_task->args = args;
    timer_task->args_size_bytes = sizeof(EpTimeoutArgs);
    timer_task->fut = NULL;
    timer_task->state = 0;
    timer_task->is_cancelled = 0;
    timer_task->parent = NULL;
    
    ep_async_register_timer(timeout_ms, timer_task);
    
    while (!fut->completed && !(args->timer_fired)) {
        if (ep_run_queue_head) {
            EpTask* task = ep_task_dequeue();
            if (task) {
                if (task->is_cancelled) {
                    if (task->fut) {
                        task->fut->completed = 1;
                        task->fut->value = -1;
                    }
                    free(task->args);
                    free(task);
                } else {
                    EpTask* saved_current = ep_current_task;
                    ep_current_task = task;
                    long long res = task->step(task->args);
                    ep_current_task = saved_current;
                    if (res != -999999) {
                        if (task->fut) {
                            task->fut->value = res;
                            task->fut->completed = 1;
                            if (task->fut->waiting_task) {
                                ep_task_enqueue(task->fut->waiting_task);
                                task->fut->waiting_task = NULL;
                            }
                        }
                        free(task->args);
                        free(task);
                    }
                }
            }
        } else {
            long long timeout = ep_get_next_timer_timeout();
            if (timeout == -1 && !ep_timers_head && ep_active_io_sources == 0) {
                break;
            }
            if (ep_event_loop_fd == -1) {
                if (timeout > 0) {
                    ep_sleep_ms(timeout);
                }
                ep_process_expired_timers();
            } else {
                ep_async_wait_step(timeout);
            }
        }
    }
    
    return fut->value;
}

/* ── Awaitable async socket-readability ─────────────────────────────────────
   `await async_wait_readable(fd)` suspends the calling async task until `fd` is
   readable, letting the event loop run other tasks (e.g. another agent waiting on
   its own LLM socket) meanwhile. Mirrors sleep_ms: build a future, register a
   oneshot read-readiness task with the loop, return the future. When fd becomes
   readable, ep_async_wait_step re-enqueues the task; its step completes the future
   and wakes whoever awaited it. This is what lets I/O-bound agents run concurrently
   on ONE thread — no OS threads, no shared-heap GC race. */
typedef struct { EpFuture* fut; } EpReadReadyArgs;
static long long ep_read_ready_step(void* r) {
    EpReadReadyArgs* args = (EpReadReadyArgs*)r;
    if (args && args->fut) {
        args->fut->completed = 1;
        args->fut->value = 1;
        if (args->fut->waiting_task) {
            ep_task_enqueue(args->fut->waiting_task);
            args->fut->waiting_task = NULL;
        }
    }
    return 0;
}
long long async_wait_readable(long long fd) {
    EpFuture* fut = (EpFuture*)malloc(sizeof(EpFuture));
    fut->completed = 0;
    fut->value = 0;
    fut->waiting_task = NULL;
    fut->chan = 0;
    { EpGCObject* _go = ep_gc_register(fut, EP_OBJ_STRUCT); if(_go) _go->num_fields = 3; }
    EpReadReadyArgs* args = (EpReadReadyArgs*)malloc(sizeof(EpReadReadyArgs));
    args->fut = fut;
    EpTask* task = (EpTask*)malloc(sizeof(EpTask));
    task->step = ep_read_ready_step;
    task->args = args;
    task->args_size_bytes = sizeof(EpReadReadyArgs);
    task->fut = NULL;
    task->state = 0;
    task->is_cancelled = 0;
    task->parent = ep_current_task;
    ep_async_register_read((int)fd, task);
    return (long long)fut;
}

typedef struct {
    EpFuture* fut;
} EpSleepTimerArgs;

static long long ep_sleep_timer_step(void* r) {
    EpSleepTimerArgs* args = (EpSleepTimerArgs*)r;
    if (args && args->fut) {
        args->fut->completed = 1;
        args->fut->value = 0;
        if (args->fut->waiting_task) {
            ep_task_enqueue(args->fut->waiting_task);
            args->fut->waiting_task = NULL;
        }
    }
    return 0;
}

static long long sleep_ms(long long ms) {
    EpFuture* fut = (EpFuture*)malloc(sizeof(EpFuture));
    fut->completed = 0;
    fut->value = 0;
    fut->waiting_task = NULL;
    fut->chan = 0;
    { EpGCObject* _go = ep_gc_register(fut, EP_OBJ_STRUCT); if(_go) _go->num_fields = 3; }
    
    EpSleepTimerArgs* args = (EpSleepTimerArgs*)malloc(sizeof(EpSleepTimerArgs));
    args->fut = fut;
    
    EpTask* task = (EpTask*)malloc(sizeof(EpTask));
    task->step = ep_sleep_timer_step;
    task->args = args;
    task->args_size_bytes = sizeof(EpSleepTimerArgs);
    task->fut = NULL;
    task->state = 0;
    task->is_cancelled = 0;
    task->parent = ep_current_task;
    
    ep_async_register_timer(ms, task);
    return (long long)fut;
}

static long long cancel_task(long long fut_ptr) {
    EpFuture* fut = (EpFuture*)fut_ptr;
    if (fut) {
        EpTask* task = ep_find_task_by_future(fut);
        if (task) {
            ep_cancel_task(task);
        } else {
            fut->completed = 1;
            fut->value = -1;
        }
    }
    return 0;
}

/* Closure environment — captures travel with the function pointer */
#define EP_CLOSURE_MAGIC 0x4550434C4FL
typedef struct {
    long long magic;
    long long fn_ptr;
    long long env[];  /* flexible array of captured values */
} EpClosure;

/* GC globals */
static EpGCObject* ep_gc_head = NULL;
static long long ep_gc_count = 0;
static long long ep_gc_threshold = 4096;
static int ep_gc_enabled = 1;
static long long ep_gc_nursery_count = 0;
static long long ep_gc_nursery_threshold = 512;
static int ep_gc_minor_count = 0;
static int ep_gc_major_count = 0;
static void** ep_gc_remembered_set = NULL;
static long long ep_gc_remembered_cap = 0;
static long long ep_gc_remembered_size = 0;
/* Single mutex for ALL GC and thread registry operations.
   Previous design had two mutexes (ep_gc_mutex + ep_thread_registry_mutex)
   which caused deadlock under concurrent channel load: thread A held gc_mutex
   and waited for registry_mutex, thread B held registry_mutex and waited for
   gc_mutex. Single lock eliminates the ordering problem. */
#ifdef __wasm__
#define __thread
#endif
static pthread_mutex_t ep_gc_mutex = PTHREAD_MUTEX_INITIALIZER;

/* Stop-the-world coordination. The collector sets ep_gc_stop_requested and, in
   ep_gc_stop_the_world(), waits until every *other* registered thread has parked
   at a safepoint (ep_gc_park_if_stopped). This guarantees mark/sweep never runs
   concurrently with a mutator changing its roots or an object's fields — the
   "marking races with running mutators" hazard. All three fields are touched
   only while holding ep_gc_mutex (the lock-free reads of ep_gc_stop_requested at
   safepoints are a benign optimization: a missed set just defers parking to the
   next safepoint, and the collector's bounded wait covers it). */
static volatile int ep_gc_stop_requested = 0;
static int ep_gc_parked_count = 0;
static pthread_cond_t ep_gc_resume_cond = PTHREAD_COND_INITIALIZER;

/* Function pointer for channel scanning — set after EpChannel is defined.
   GC mark calls this to scan values in-transit in channel buffers. */
static void (*ep_gc_scan_channels_major)(void) = NULL;
static void (*ep_gc_scan_channels_minor)(void) = NULL;
/* Function pointers for marking top-level constant/global variables, which are
   GC roots that live outside any function frame. Set by __ep_init_constants. */
static void (*ep_gc_mark_globals_major)(void) = NULL;
static void (*ep_gc_mark_globals_minor)(void) = NULL;
/* Function pointers for map value traversal — set after EpMap is defined.
   GC mark calls these to recursively mark values stored in maps. */
static void (*ep_gc_mark_map_values)(void* ptr) = NULL;
static void (*ep_gc_mark_map_values_minor)(void* ptr) = NULL;

/* Thread registry for GC root scanning in multi-threaded environment */
#define EP_MAX_THREADS 256
static __thread void* volatile ep_thread_local_top = NULL;
static __thread void* ep_thread_local_bottom = NULL;

static void* volatile* ep_thread_tops[EP_MAX_THREADS];
static void* ep_thread_bottoms[EP_MAX_THREADS];
static volatile int ep_thread_active[EP_MAX_THREADS];
static int ep_num_threads = 0;

/* Per-thread GC root state — heap-allocated, stable across thread lifetime.
   Previous design stored raw pointers to __thread arrays (ep_gc_root_stack,
   ep_gc_root_sp) in the global registry. When a thread exited, the __thread
   storage was freed, leaving dangling pointers that ep_gc_mark would
   dereference → segfault. Now each thread gets a heap-allocated state struct
   that survives thread exit and is only recycled when the slot is reused. */
typedef struct {
    long long* roots[4096];  /* copy of root pointers, updated under lock */
    volatile int sp;         /* current root stack pointer */
} EpThreadGCState;

static EpThreadGCState* ep_thread_gc_states[EP_MAX_THREADS];

/* Shadow stack for explicit GC roots — thread-local to prevent cross-thread corruption */
#define EP_GC_MAX_ROOTS 4096
static __thread long long* ep_gc_root_stack[EP_GC_MAX_ROOTS];
static __thread int ep_gc_root_sp = 0;
static __thread int ep_thread_slot = -1;

/* ep_gc_root_sp is the *logical* shadow-stack depth. It always advances on
   push and retreats on pop so that per-frame push/pop counts stay balanced.
   Array storage is capped at EP_GC_MAX_ROOTS: once the stack is full, further
   roots are counted but not stored (those deep-overflow locals are simply not
   traced) — crucially, we never overwrite or drop an outer frame's stored
   roots, which the old "silently skip the push but still pop" path did. */
static void ep_gc_push_root(long long* root) {
    int idx = ep_gc_root_sp;
    ep_gc_root_sp++;
    if (idx < EP_GC_MAX_ROOTS) {
        ep_gc_root_stack[idx] = root;
        /* Update the heap-allocated state so GC mark can see it safely */
        if (ep_thread_slot >= 0 && ep_thread_gc_states[ep_thread_slot]) {
            ep_thread_gc_states[ep_thread_slot]->roots[idx] = root;
            ep_thread_gc_states[ep_thread_slot]->sp =
                (ep_gc_root_sp < EP_GC_MAX_ROOTS) ? ep_gc_root_sp : EP_GC_MAX_ROOTS;
        }
    }
}
static void ep_gc_pop_roots(long long count) {
    ep_gc_root_sp -= (int)count;
    if (ep_gc_root_sp < 0) ep_gc_root_sp = 0;
    /* Update the heap-allocated state (clamped to the array bound) */
    if (ep_thread_slot >= 0 && ep_thread_gc_states[ep_thread_slot]) {
        ep_thread_gc_states[ep_thread_slot]->sp =
            (ep_gc_root_sp < EP_GC_MAX_ROOTS) ? ep_gc_root_sp : EP_GC_MAX_ROOTS;
    }
}

/* Park the calling thread if the collector has stopped the world.
   MUST be called with ep_gc_mutex held. The thread's shadow stack (its precise
   root set) is stable while parked, so the collector can scan it race-free. */
static void ep_gc_park_if_stopped(void) {
    if (!ep_gc_stop_requested) return;
    /* Spill registers onto the stack and publish this thread's current stack top
       so the collector can conservatively scan its frozen C stack while parked —
       this catches roots held only in registers/temporaries that the precise
       shadow stack does not yet record. _dummy is declared below _pregs, so its
       (lower) address bounds a scan range that covers the spilled registers. */
    jmp_buf _pregs;
    volatile char _top_marker;  /* function-scope: stays valid while parked */
    memset(&_pregs, 0, sizeof(_pregs));
    setjmp(_pregs);
    /* _top_marker is declared after _pregs, so its (lower) address bounds a scan
       range [&_top_marker, stack_bottom] that covers the spilled registers. */
    ep_thread_local_top = (void*)&_top_marker;
    __sync_synchronize();  /* publish shadow-stack + top writes before parking */
    ep_gc_parked_count++;
    while (ep_gc_stop_requested) {
        pthread_cond_wait(&ep_gc_resume_cond, &ep_gc_mutex);
    }
    ep_gc_parked_count--;
}

/* Begin a stop-the-world pause. MUST be called with ep_gc_mutex held.
   Waits (briefly releasing the lock so blocked mutators can reach a safepoint)
   until all other registered threads have parked. After a bounded fallback
   (~50ms) it proceeds anyway: any thread that hasn't parked by then is blocked
   or idle with a stable shadow stack, so scanning it is still safe in practice. */
static void ep_gc_stop_the_world(void) {
    ep_gc_stop_requested = 1;
    /* Actively-running threads reach a safepoint (every allocation and every
       function entry) within microseconds, so they park on the first spin or
       two. The bound only caps the rare case where a thread is blocked/idle
       (e.g. just entered a channel op) and won't park — those have a stable
       shadow stack, so proceeding to scan them is safe. ~40 * 250us ≈ 10ms. */
    for (int spins = 0; spins < 40; spins++) {
        int others = 0;
        for (int t = 0; t < ep_num_threads; t++) {
            if (ep_thread_active[t] && t != ep_thread_slot) others++;
        }
        if (others <= 0 || ep_gc_parked_count >= others) return;
        pthread_mutex_unlock(&ep_gc_mutex);
#ifdef _WIN32
        Sleep(1);
#elif !defined(__wasm__)
        usleep(250);
#endif
        pthread_mutex_lock(&ep_gc_mutex);
    }
}

/* End a stop-the-world pause and wake all parked threads. MUST hold ep_gc_mutex. */
static void ep_gc_start_the_world(void) {
    ep_gc_stop_requested = 0;
    pthread_cond_broadcast(&ep_gc_resume_cond);
}

static void ep_gc_register_thread(void* stack_bottom) {
    ep_thread_local_bottom = stack_bottom;
    ep_thread_local_top = stack_bottom;
    
    pthread_mutex_lock(&ep_gc_mutex);
    int slot = -1;
    for (int i = 0; i < ep_num_threads; i++) {
        if (!ep_thread_active[i]) {
            slot = i;
            break;
        }
    }
    if (slot == -1 && ep_num_threads < EP_MAX_THREADS) {
        slot = ep_num_threads++;
    }
    if (slot != -1) {
        ep_thread_tops[slot] = &ep_thread_local_top;
        ep_thread_bottoms[slot] = stack_bottom;
        /* Allocate or reuse heap state for this slot */
        if (!ep_thread_gc_states[slot]) {
            ep_thread_gc_states[slot] = (EpThreadGCState*)calloc(1, sizeof(EpThreadGCState));
        }
        ep_thread_gc_states[slot]->sp = 0;
        ep_thread_slot = slot;
        __sync_synchronize();  /* Memory barrier: state must be visible before active */
        ep_thread_active[slot] = 1;
    }
    pthread_mutex_unlock(&ep_gc_mutex);
}

static void ep_gc_unregister_thread(void) {
    pthread_mutex_lock(&ep_gc_mutex);
    for (int i = 0; i < ep_num_threads; i++) {
        if (ep_thread_active[i] && ep_thread_tops[i] == &ep_thread_local_top) {
            /* Zero root count FIRST — even if ep_gc_mark races past the
               active check, it will see sp=0 and walk no roots instead
               of dereferencing stale __thread pointers */
            if (ep_thread_gc_states[i]) {
                ep_thread_gc_states[i]->sp = 0;
            }
            __sync_synchronize();  /* Memory barrier: sp=0 visible before deactivation */
            ep_thread_active[i] = 0;
            ep_thread_slot = -1;
            break;
        }
    }
    pthread_mutex_unlock(&ep_gc_mutex);
}

#define EP_GC_UPDATE_TOP() { volatile int _dummy; ep_thread_local_top = (void*)&_dummy; }

/* Simple open-addressed hash map with linear probing for O(1) GC object lookup */
typedef struct {
    void* key;
    EpGCObject* value;
} EpGCEntry;

static EpGCEntry* ep_gc_table = NULL;
static long long ep_gc_table_cap = 0;
static long long ep_gc_table_size = 0;

/* Bucket index for a pointer key. The previous hash was ((uintptr_t)key % cap)
   with cap a power of two; malloc returns 16-byte-aligned pointers, so the low 4
   bits are always 0 and only every 16th bucket was ever a home slot. That caused
   catastrophic primary clustering -> O(n) probe runs -> ep_gc_table_remove's
   rehash became O(n^2), which (under the single global GC mutex) wedged the whole
   node when a large object list was freed. A splitmix64 finalizer avalanches all
   bits, so even the low bits taken by the (cap-1) mask are well distributed. */
static inline long long ep_gc_index(void* key, long long cap) {
    uint64_t z = (uint64_t)(uintptr_t)key;
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
    z = z ^ (z >> 31);
    return (long long)(z & (uint64_t)(cap - 1));   /* cap is always a power of two */
}

/* Insert without growing — assumes a free slot exists. Used by the resize and by
   ep_gc_table_remove's rehash, neither of which may trigger a (re)allocation of
   the table mid-iteration. */
static void ep_gc_table_place(void* key, EpGCObject* value) {
    long long idx = ep_gc_index(key, ep_gc_table_cap);
    while (ep_gc_table[idx].key != NULL) {
        if (ep_gc_table[idx].key == key) {
            ep_gc_table[idx].value = value;
            return;
        }
        idx = (idx + 1) & (ep_gc_table_cap - 1);
    }
    ep_gc_table[idx].key = key;
    ep_gc_table[idx].value = value;
    ep_gc_table_size++;
}

static void ep_gc_table_insert(void* key, EpGCObject* value) {
    if (ep_gc_table_size * 2 >= ep_gc_table_cap) {
        long long old_cap = ep_gc_table_cap;
        long long new_cap = old_cap == 0 ? 512 : old_cap * 2;
        EpGCEntry* new_table = (EpGCEntry*)calloc(new_cap, sizeof(EpGCEntry));
        EpGCEntry* old_table = ep_gc_table;
        ep_gc_table = new_table;
        ep_gc_table_cap = new_cap;
        ep_gc_table_size = 0;
        for (long long i = 0; i < old_cap; i++) {
            if (old_table[i].key != NULL) {
                ep_gc_table_place(old_table[i].key, old_table[i].value);
            }
        }
        free(old_table);
    }
    ep_gc_table_place(key, value);
}

static EpGCObject* ep_gc_table_get(void* key) {
    if (ep_gc_table_cap == 0) return NULL;
    long long idx = ep_gc_index(key, ep_gc_table_cap);
    while (ep_gc_table[idx].key != NULL) {
        if (ep_gc_table[idx].key == key) return ep_gc_table[idx].value;
        idx = (idx + 1) & (ep_gc_table_cap - 1);
    }
    return NULL;
}

static void ep_gc_table_remove(void* key) {
    if (ep_gc_table_cap == 0) return;
    long long idx = ep_gc_index(key, ep_gc_table_cap);
    while (ep_gc_table[idx].key != NULL) {
        if (ep_gc_table[idx].key == key) {
            ep_gc_table[idx].key = NULL;
            ep_gc_table[idx].value = NULL;
            ep_gc_table_size--;
            /* Backward-shift rehash of the rest of this cluster. Re-place (no
               resize: size is not growing) so a mid-iteration realloc can never
               free the table out from under this loop. */
            long long next_idx = (idx + 1) & (ep_gc_table_cap - 1);
            while (ep_gc_table[next_idx].key != NULL) {
                void* rehash_key = ep_gc_table[next_idx].key;
                EpGCObject* rehash_val = ep_gc_table[next_idx].value;
                ep_gc_table[next_idx].key = NULL;
                ep_gc_table[next_idx].value = NULL;
                ep_gc_table_size--;
                ep_gc_table_place(rehash_key, rehash_val);
                next_idx = (next_idx + 1) & (ep_gc_table_cap - 1);
            }
            return;
        }
        idx = (idx + 1) & (ep_gc_table_cap - 1);
    }
}



/* Register a new GC object */
static EpGCObject* ep_gc_register(void* ptr, EpObjKind kind) {
    if (!ptr) return NULL;
    pthread_mutex_lock(&ep_gc_mutex);
    ep_gc_park_if_stopped();  /* safepoint: don't allocate/touch the table mid-collection */
    EpGCObject* obj = (EpGCObject*)malloc(sizeof(EpGCObject));
    if (!obj) {
        pthread_mutex_unlock(&ep_gc_mutex);
        return NULL;
    }
    obj->kind = kind;
    obj->marked = 0;
    obj->ptr = ptr;
    obj->size = 0;
    obj->num_fields = 0;
    obj->generation = 0;
    obj->next = ep_gc_head;
    ep_gc_head = obj;
    ep_gc_count++;
    ep_gc_nursery_count++;
    ep_gc_table_insert(ptr, obj);
    pthread_mutex_unlock(&ep_gc_mutex);
    return obj;
}

/* Find GC object by pointer.
   Takes ep_gc_mutex because ep_gc_table_insert may realloc+free the table
   concurrently (from another thread's allocation). Mutator-side callers
   (write barrier, free_struct/free_map/free_list, to-string) must use this
   locking variant; code already holding the mutex (mark/sweep) calls
   ep_gc_table_get directly to avoid a non-recursive double-lock deadlock. */
static EpGCObject* ep_gc_find(void* ptr) {
    pthread_mutex_lock(&ep_gc_mutex);
    ep_gc_park_if_stopped();  /* safepoint */
    EpGCObject* obj = ep_gc_table_get(ptr);
    pthread_mutex_unlock(&ep_gc_mutex);
    return obj;
}

/* Write barrier for generational GC: tracks references from old objects (gen 1) to young objects (gen 0).
   The whole operation runs under ep_gc_mutex so the table lookups and the
   remembered-set update see a consistent table (no race with a concurrent
   resize) and use the no-lock ep_gc_table_get to avoid re-entering the lock. */
static void ep_gc_write_barrier(void* host_ptr, long long val) {
    if (val == 0) return;
    pthread_mutex_lock(&ep_gc_mutex);
    ep_gc_park_if_stopped();  /* safepoint: don't update the remembered set mid-collection */
    EpGCObject* host_obj = ep_gc_table_get(host_ptr);
    EpGCObject* val_obj = ep_gc_table_get((void*)val);
    if (host_obj && val_obj && host_obj->generation == 1 && val_obj->generation == 0) {
        /* Check if already in remembered set */
        int found = 0;
        for (long long i = 0; i < ep_gc_remembered_size; i++) {
            if (ep_gc_remembered_set[i] == (void*)val) {
                found = 1;
                break;
            }
        }
        if (!found) {
            if (ep_gc_remembered_size >= ep_gc_remembered_cap) {
                long long new_cap = ep_gc_remembered_cap == 0 ? 128 : ep_gc_remembered_cap * 2;
                void** new_set = (void**)realloc(ep_gc_remembered_set, new_cap * sizeof(void*));
                if (new_set) {
                    ep_gc_remembered_set = new_set;
                    ep_gc_remembered_cap = new_cap;
                }
            }
            if (ep_gc_remembered_size < ep_gc_remembered_cap) {
                ep_gc_remembered_set[ep_gc_remembered_size++] = (void*)val;
            }
        }
    }
    pthread_mutex_unlock(&ep_gc_mutex);
}

/* Forward declarations for list type (needed by GC mark) */
typedef struct {
    long long* data;
    long long length;
    long long capacity;
} EpList;

/* A real heap object (list/map/string) is malloc'd, so its address is far above
   the never-mapped first page. EP values that are NOT pointers — small ints,
   booleans, and JSON type-tags (2=string, 3=list, 4=object) — land in [0,4096).
   Guarding the object accessors with this turns "deref a non-pointer" (the cause
   of the read_transcripts segfault, and that whole class) into a safe null return
   instead of a daemon-killing SIGSEGV. One comparison; negligible on hot paths. */
#define EP_BADPTR(p) (((unsigned long long)(p)) < 4096ULL)

/* Mark a single object and recursively mark its children */
static void ep_gc_mark_object(void* ptr) {
    if (!ptr) return;
    /* Runs under ep_gc_mutex (held by the collector) — use the no-lock lookup. */
    EpGCObject* obj = ep_gc_table_get(ptr);
    if (!obj || obj->marked) return;
    obj->marked = 1;

    if (obj->kind == EP_OBJ_LIST) {
        EpList* list = (EpList*)ptr;
        for (long long i = 0; i < list->length; i++) {
            long long val = list->data[i];
            if (val != 0) {
                ep_gc_mark_object((void*)val);
            }
        }
    } else if (obj->kind == EP_OBJ_STRUCT) {
        long long* fields = (long long*)ptr;
        for (long long i = 0; i < obj->num_fields; i++) {
            if (fields[i] != 0) {
                ep_gc_mark_object((void*)fields[i]);
            }
        }
    } else if (obj->kind == EP_OBJ_MAP) {
        if (ep_gc_mark_map_values) ep_gc_mark_map_values(ptr);
    }
}

/* Mark a single object and recursively mark its children (only if it is Gen 0) */
static void ep_gc_mark_object_minor(void* ptr) {
    if (!ptr) return;
    /* Runs under ep_gc_mutex (held by the collector) — use the no-lock lookup. */
    EpGCObject* obj = ep_gc_table_get(ptr);
    if (!obj || obj->generation != 0 || obj->marked) return;
    obj->marked = 1;

    if (obj->kind == EP_OBJ_LIST) {
        EpList* list = (EpList*)ptr;
        for (long long i = 0; i < list->length; i++) {
            long long val = list->data[i];
            if (val != 0) {
                ep_gc_mark_object_minor((void*)val);
            }
        }
    } else if (obj->kind == EP_OBJ_STRUCT) {
        long long* fields = (long long*)ptr;
        for (long long i = 0; i < obj->num_fields; i++) {
            if (fields[i] != 0) {
                ep_gc_mark_object_minor((void*)fields[i]);
            }
        }
    } else if (obj->kind == EP_OBJ_MAP) {
        if (ep_gc_mark_map_values_minor) ep_gc_mark_map_values_minor(ptr);
    }
}

/* Conservatively scan every registered thread's C stack and mark any word that
   looks like a tracked pointer. The collector spills its own registers and
   publishes its top here; all other threads are parked at a safepoint with their
   registers spilled and top published (ep_gc_park_if_stopped), so their stacks
   are frozen. This complements the precise shadow stacks: it catches roots held
   only in registers/temporaries (e.g. a freshly allocated object not yet stored
   into a rooted slot). Non-pointer words are harmlessly ignored by ep_gc_find.

   Only run on MAJOR collections: minor collections rely on the precise shadow
   stacks plus the write barrier's remembered set (the standard generational
   approach), so they do no stack scan at all — which means there is no racy
   cross-thread stack read on the frequent minor path either. The expensive
   full-stack scan is paid only on the rarer major collection, where it pins
   any long-lived object reachable only via a register across many GCs.

   Marked no_sanitize_address: a conservative scan deliberately reads whole stack
   ranges (including ASAN redzones and out-of-frame slots), which is not a bug. */
#if defined(__SANITIZE_ADDRESS__)
# define EP_NO_ASAN __attribute__((no_sanitize_address))
#elif defined(__has_feature)
# if __has_feature(address_sanitizer)
#  define EP_NO_ASAN __attribute__((no_sanitize_address))
# endif
#endif
#ifndef EP_NO_ASAN
# define EP_NO_ASAN
#endif
EP_NO_ASAN
static void ep_gc_scan_thread_stacks(void) {
    jmp_buf _regs;
    volatile char _top_marker;
    memset(&_regs, 0, sizeof(_regs));
    setjmp(_regs);   /* spill the collector's own registers onto its stack */
    /* Publish the LOWEST of our own local addresses as this thread's live top, so the
       scanned range covers both the stack marker and the register-spill buffer whatever
       order the compiler laid them out (a missed _regs would drop a register-only root). */
    { char* _a = (char*)(void*)&_top_marker; char* _b = (char*)(void*)&_regs;
      ep_thread_local_top = (void*)((_a < _b) ? _a : _b); }
    for (int t = 0; t < ep_num_threads; t++) {
        if (!ep_thread_active[t]) continue;
        if (!ep_thread_tops[t]) continue;
        /* The published top comes from a char local, so it may not be pointer-aligned;
           mask DOWN to 8 bytes. Aligning down only widens the conservative window by a
           few harmless bytes — aligning up could skip the slot holding a live root.
           Unaligned void** dereferences are UB and produce a skewed scan window on
           strict platforms (caught by valgrind on Linux). */
        void** start = (void**)((uintptr_t)*ep_thread_tops[t] & ~(uintptr_t)7);
        void** end = (void**)ep_thread_bottoms[t];
        if (!start || !end) continue;
        if (start > end) { void** tmp = start; start = end; end = tmp; }
        for (void** cur = start; cur < end; cur++) {
            void* p = *cur;
            if (p) ep_gc_mark_object(p);
        }
    }
}

/* Mark phase: traverse from ALL threads' explicit GC roots.
   Uses the heap-allocated EpThreadGCState instead of raw __thread pointers. */
static void ep_gc_mark(void) {
    ep_gc_scan_thread_stacks();  /* conservative C-stack scan of all (parked) threads — major only */
    for (int t = 0; t < ep_num_threads; t++) {
        if (!ep_thread_active[t]) continue;
        EpThreadGCState* state = ep_thread_gc_states[t];
        if (!state) continue;
        int sp = state->sp;
        if (sp <= 0 || sp > EP_GC_MAX_ROOTS) continue;
        for (int i = 0; i < sp; i++) {
            long long* root_ptr = state->roots[i];
            if (!root_ptr) continue;
            long long val = *root_ptr;
            if (val != 0) {
                ep_gc_mark_object((void*)val);
            }
        }
    }
    /* Also mark from main thread's local root stack (thread 0 / unregistered) */
    int local_sp = ep_gc_root_sp;
    if (local_sp > EP_GC_MAX_ROOTS) local_sp = EP_GC_MAX_ROOTS;
    for (int i = 0; i < local_sp; i++) {
        long long val = *ep_gc_root_stack[i];
        if (val != 0) {
            ep_gc_mark_object((void*)val);
        }
    }
    /* Mark active tasks in the scheduler run queue */
    EpTask* task = ep_run_queue_head;
    while (task) {
        if (task->fut) {
            ep_gc_mark_object((void*)task->fut);
        }
        if (task->args && task->args_size_bytes > 0) {
            long long* ptr = (long long*)task->args;
            for (int i = 0; i < task->args_size_bytes / 8; i++) {
                long long val = ptr[i];
                if (val != 0) ep_gc_mark_object((void*)val);
            }
        }
        task = task->next;
    }
    /* Mark active tasks in the timers queue */
    EpTimer* timer = ep_timers_head;
    while (timer) {
        if (timer->task) {
            EpTask* t = timer->task;
            if (t->fut) {
                ep_gc_mark_object((void*)t->fut);
            }
            if (t->args && t->args_size_bytes > 0) {
                long long* ptr = (long long*)t->args;
                for (int i = 0; i < t->args_size_bytes / 8; i++) {
                    long long val = ptr[i];
                    if (val != 0) ep_gc_mark_object((void*)val);
                }
            }
        }
        timer = timer->next;
    }
    /* Mark top-level constant/global variables (roots outside any frame) */
    if (ep_gc_mark_globals_major) ep_gc_mark_globals_major();
    /* Scan all registered channel buffers — values in-transit have no root */
    if (ep_gc_scan_channels_major) ep_gc_scan_channels_major();
}

/* Conservatively scan the CURRENT thread's own live C stack and mark any YOUNG object it
   finds. This closes a use-after-free on the frequent minor path: a freshly-allocated
   argument temporary — e.g. the result of g() while f(g() and h()) is still evaluating
   h() — lives only on the C stack / in registers and is not yet on the precise shadow
   stack, so a minor collection triggered mid-expression would otherwise free it. Scanning
   ONLY the collecting thread's own stack is race-free (no cross-thread read) and cheap
   (one bounded stack, current thread only). Non-pointer words are harmlessly ignored by
   ep_gc_table_get; only generation-0 objects are marked. The setjmp spills register-held
   roots onto the stack so the scan can see them. */
EP_NO_ASAN
static void ep_gc_scan_own_stack_minor(void) {
    jmp_buf _regs;
    volatile char _marker;
    memset(&_regs, 0, sizeof(_regs));
    setjmp(_regs);   /* spill callee-saved registers into _regs, on the stack */
    void* bottom = ep_thread_local_bottom;
    if (!bottom) return;
    /* Start at the LOWEST of our own local addresses so the scanned range covers both
       the current stack top (_marker) and the register-spill buffer (_regs), regardless
       of how the compiler ordered these locals on the stack. Missing _regs would drop a
       root held only in a callee-saved register -> a rare use-after-free. */
    char* a = (char*)(void*)&_marker;
    char* b = (char*)(void*)&_regs;
    char* lo = (a < b) ? a : b;
    /* lo comes from a char local, so it may not be pointer-aligned; mask DOWN to 8
       bytes. Aligning down only widens the conservative window by a few harmless
       bytes — aligning up could skip the slot holding a live root. Unaligned void**
       dereferences are UB and skew the scan window on strict platforms (valgrind). */
    void** start = (void**)((uintptr_t)lo & ~(uintptr_t)7);
    void** end = (void**)bottom;
    if (start > end) { void** tmp = start; start = end; end = tmp; }
    for (void** cur = start; cur < end; cur++) {
        void* p = *cur;
        if (p) ep_gc_mark_object_minor(p);
    }
}

static void ep_gc_mark_minor(void) {
    /* Conservatively scan our OWN live C stack first, to catch freshly-allocated argument
       temporaries (only on the stack / in registers, not yet on the shadow stack) that a
       minor collection mid-expression would otherwise free. Own-thread only, so race-free. */
    ep_gc_scan_own_stack_minor();
    for (int t = 0; t < ep_num_threads; t++) {
        if (!ep_thread_active[t]) continue;
        EpThreadGCState* state = ep_thread_gc_states[t];
        if (!state) continue;
        int sp = state->sp;
        if (sp <= 0 || sp > EP_GC_MAX_ROOTS) continue;
        for (int i = 0; i < sp; i++) {
            long long* root_ptr = state->roots[i];
            if (!root_ptr) continue;
            long long val = *root_ptr;
            if (val != 0) {
                ep_gc_mark_object_minor((void*)val);
            }
        }
    }
    int local_sp = ep_gc_root_sp;
    if (local_sp > EP_GC_MAX_ROOTS) local_sp = EP_GC_MAX_ROOTS;
    for (int i = 0; i < local_sp; i++) {
        long long val = *ep_gc_root_stack[i];
        if (val != 0) {
            ep_gc_mark_object_minor((void*)val);
        }
    }
    /* Mark active tasks in the scheduler run queue for minor collection */
    EpTask* task = ep_run_queue_head;
    while (task) {
        if (task->fut) {
            ep_gc_mark_object_minor((void*)task->fut);
        }
        if (task->args && task->args_size_bytes > 0) {
            long long* ptr = (long long*)task->args;
            for (int i = 0; i < task->args_size_bytes / 8; i++) {
                long long val = ptr[i];
                if (val != 0) ep_gc_mark_object_minor((void*)val);
            }
        }
        task = task->next;
    }
    /* Mark active tasks in the timers queue for minor collection */
    EpTimer* timer = ep_timers_head;
    while (timer) {
        if (timer->task) {
            EpTask* t = timer->task;
            if (t->fut) {
                ep_gc_mark_object_minor((void*)t->fut);
            }
            if (t->args && t->args_size_bytes > 0) {
                long long* ptr = (long long*)t->args;
                for (int i = 0; i < t->args_size_bytes / 8; i++) {
                    long long val = ptr[i];
                    if (val != 0) ep_gc_mark_object_minor((void*)val);
                }
            }
        }
        timer = timer->next;
    }
    /* Also mark from the remembered set */
    for (long long i = 0; i < ep_gc_remembered_size; i++) {
        ep_gc_mark_object_minor(ep_gc_remembered_set[i]);
    }
    /* Mark top-level constant/global variables (roots outside any frame) */
    if (ep_gc_mark_globals_minor) ep_gc_mark_globals_minor();
    /* Scan all registered channel buffers — values in-transit have no root */
    if (ep_gc_scan_channels_minor) ep_gc_scan_channels_minor();
}

static void ep_gc_sweep_minor(void) {
    EpGCObject** cur = &ep_gc_head;
    while (*cur) {
        if ((*cur)->generation == 0) {
            if (!(*cur)->marked) {
                EpGCObject* garbage = *cur;
                *cur = garbage->next;
                ep_gc_table_remove(garbage->ptr);
                if (garbage->kind == EP_OBJ_LIST) {
                    EpList* list = (EpList*)garbage->ptr;
                    if (list) {
                        free(list->data);
                        free(list);
                    }
                } else if (garbage->kind == EP_OBJ_STRING) {
                    free(garbage->ptr);
                } else if (garbage->kind == EP_OBJ_STRUCT) {
                    free(garbage->ptr);
                } else if (garbage->kind == EP_OBJ_CLOSURE) {
                    free(garbage->ptr);
                } else if (garbage->kind == EP_OBJ_MAP) {
                    /* EpMap layout: entries*, capacity, size. Free entries then map. */
                    void** map_fields = (void**)garbage->ptr;
                    if (map_fields && map_fields[0]) free(map_fields[0]); /* entries */
                    free(garbage->ptr);
                }
                free(garbage);
                ep_gc_count--;
                ep_gc_nursery_count--;
            } else {
                (*cur)->marked = 0;
                (*cur)->generation = 1;
                ep_gc_nursery_count--;
                cur = &(*cur)->next;
            }
        } else {
            cur = &(*cur)->next;
        }
    }
    ep_gc_remembered_size = 0;
}

static void ep_gc_sweep_major(void) {
    EpGCObject** cur = &ep_gc_head;
    while (*cur) {
        if (!(*cur)->marked) {
            EpGCObject* garbage = *cur;
            *cur = garbage->next;
            ep_gc_table_remove(garbage->ptr);
            if (garbage->generation == 0) {
                ep_gc_nursery_count--;
            }
            if (garbage->kind == EP_OBJ_LIST) {
                EpList* list = (EpList*)garbage->ptr;
                if (list) {
                    free(list->data);
                    free(list);
                }
            } else if (garbage->kind == EP_OBJ_STRING) {
                free(garbage->ptr);
            } else if (garbage->kind == EP_OBJ_STRUCT) {
                free(garbage->ptr);
            } else if (garbage->kind == EP_OBJ_CLOSURE) {
                free(garbage->ptr);
            } else if (garbage->kind == EP_OBJ_MAP) {
                void** map_fields = (void**)garbage->ptr;
                if (map_fields && map_fields[0]) free(map_fields[0]);
                free(garbage->ptr);
            }
            free(garbage);
            ep_gc_count--;
        } else {
            (*cur)->marked = 0;
            if ((*cur)->generation == 0) {
                (*cur)->generation = 1;
                ep_gc_nursery_count--;
            }
            cur = &(*cur)->next;
        }
    }
    ep_gc_remembered_size = 0;
}

static void ep_gc_collect_minor(void) {
    if (!ep_gc_enabled) return;
    ep_gc_minor_count++;
    ep_gc_mark_minor();
    ep_gc_sweep_minor();
}

static void ep_gc_collect_major(void) {
    if (!ep_gc_enabled) return;
    ep_gc_major_count++;
    ep_gc_mark();
    ep_gc_sweep_major();
    ep_gc_threshold = ep_gc_count * 2;
    if (ep_gc_threshold < 4096) ep_gc_threshold = 4096;
}

/* Run a full GC collection — caller MUST hold ep_gc_mutex */
static void ep_gc_collect(void) {
    ep_gc_collect_major();
}

/* Maybe trigger GC if we've exceeded threshold. Also serves as the per-function
   GC safepoint: if another thread has stopped the world, park here until it's done. */
static void ep_gc_maybe_collect(void) {
    if (!ep_gc_enabled) return;  /* Early exit if GC suppressed (e.g. during channel ops) */
    /* Safepoint: lock-free fast check, then park under the lock if a collection
       is in progress on another thread. Keeps the no-GC path lock-free. */
    if (ep_gc_stop_requested) {
        pthread_mutex_lock(&ep_gc_mutex);
        ep_gc_park_if_stopped();
        pthread_mutex_unlock(&ep_gc_mutex);
    }
    /* Fast path: check thresholds before acquiring mutex.
       Counters are only incremented under the mutex, so worst case
       we miss one collection cycle — safe trade-off for avoiding
       a mutex lock/unlock (~20-50ns) on every function call. */
    if (ep_gc_nursery_count < ep_gc_nursery_threshold && ep_gc_count < ep_gc_threshold) return;
    EP_GC_UPDATE_TOP();
    pthread_mutex_lock(&ep_gc_mutex);
    /* Another thread may have started collecting between the check and the lock —
       park instead of racing it, then re-check thresholds under the lock. */
    ep_gc_park_if_stopped();
    if (ep_gc_nursery_count >= ep_gc_nursery_threshold || ep_gc_count >= ep_gc_threshold) {
        ep_gc_stop_the_world();
        if (ep_gc_nursery_count >= ep_gc_nursery_threshold) {
            ep_gc_collect_minor();
        }
        if (ep_gc_count >= ep_gc_threshold) {
            ep_gc_collect_major();
        }
        ep_gc_start_the_world();
    }
    pthread_mutex_unlock(&ep_gc_mutex);
}

/* Unregister an object (for explicit free — removes from GC tracking) */
static void ep_gc_unregister(void* ptr) {
    if (!ptr) return;
    pthread_mutex_lock(&ep_gc_mutex);
    ep_gc_park_if_stopped();  /* safepoint: don't mutate the table mid-collection */
    /* Clean up references from the remembered set to prevent dangling pointers */
    for (long long i = 0; i < ep_gc_remembered_size; ) {
        if (ep_gc_remembered_set[i] == ptr) {
            for (long long j = i; j < ep_gc_remembered_size - 1; j++) {
                ep_gc_remembered_set[j] = ep_gc_remembered_set[j + 1];
            }
            ep_gc_remembered_size--;
        } else {
            i++;
        }
    }
    ep_gc_table_remove(ptr);
    EpGCObject** cur = &ep_gc_head;
    while (*cur) {
        if ((*cur)->ptr == ptr) {
            EpGCObject* found = *cur;
            *cur = found->next;
            if (found->generation == 0) {
                ep_gc_nursery_count--;
            }
            free(found);
            ep_gc_count--;
            pthread_mutex_unlock(&ep_gc_mutex);
            return;
        }
        cur = &(*cur)->next;
    }
    pthread_mutex_unlock(&ep_gc_mutex);
}

/* Cleanup all remaining GC objects (called at program exit) */
static void ep_gc_shutdown(void) {
    ep_gc_enabled = 0;
    /* Only free GC bookkeeping structures, not the tracked objects themselves.
       The RAII auto-cleanup has already freed owned objects, and the OS will
       reclaim everything else on process exit. Attempting to free individual
       objects here causes double-free aborts when RAII and GC both track
       the same allocation. */
    EpGCObject* cur = ep_gc_head;
    while (cur) {
        EpGCObject* next = cur->next;
        free(cur);  /* free the GCObject wrapper only */
        cur = next;
    }
    ep_gc_head = NULL;
    ep_gc_count = 0;
    if (ep_gc_table) {
        free(ep_gc_table);
        ep_gc_table = NULL;
    }
    ep_gc_table_cap = 0;
    ep_gc_table_size = 0;
}

/* ========== End Garbage Collector ========== */

long long create_list(void);
long long append_list(long long list_ptr, long long value);
long long get_list(long long list_ptr, long long index);
long long set_list(long long list_ptr, long long index, long long value);
long long length_list(long long list_ptr);
long long free_list(long long list_ptr);
long long pop_list(long long list_ptr);
long long remove_list(long long list_ptr, long long index);
char* string_from_list(long long list_ptr);
long long string_to_list(const char* s);
long long string_length(const char* s);
long long display_string(const char* s);
long long file_read(long long path_val);
long long file_write(long long path_val, long long content_val);
long long file_append(long long path_val, long long content_val);
long long file_exists(long long path_val);
long long string_contains(long long s_val, long long sub_val);
long long string_index_of(long long s_val, long long sub_val);
long long string_replace(long long s_val, long long old_val, long long new_val);
long long string_upper(long long s_val);
long long string_lower(long long s_val);
long long string_trim(long long s_val);
long long string_split(long long s_val, long long delim_val);
long long char_at(long long s_val, long long index);
long long char_from_code(long long code);
long long ep_abs(long long n);
long long json_get_string(long long json_val, long long key_val);
long long json_get_int(long long json_val, long long key_val);
long long json_get_bool(long long json_val, long long key_val);
long long ep_sha1(long long data_val);
long long ep_net_recv_bytes(long long fd, long long count);
long long channel_try_recv(long long chan_ptr, long long out_ptr);
long long channel_has_data(long long chan_ptr);
long long channel_select(long long channels_list, long long timeout_ms);
long long ep_auto_to_string(long long val);

typedef struct EpChannel_ {
    long long* data;
    long long capacity;
    long long head;
    long long tail;
    long long size;
    ep_mutex_t mutex;
    ep_cond_t cond_recv;
    ep_cond_t cond_send;
} EpChannel;

/* Global channel registry — allows GC to scan values in-transit in channel buffers.
   Without this, an object sent to a channel but not yet received has NO GC root:
   the sender has popped it, the receiver hasn't pushed it, and the channel buffer
   is not scanned. The GC sweeps it → receiver gets a dangling pointer. */
#define EP_MAX_CHANNELS 1024
static EpChannel* ep_channel_registry[EP_MAX_CHANNELS];
static int ep_channel_count = 0;
static pthread_mutex_t ep_channel_registry_mutex = PTHREAD_MUTEX_INITIALIZER;

static void ep_register_channel(EpChannel* chan) {
    pthread_mutex_lock(&ep_channel_registry_mutex);
    if (ep_channel_count < EP_MAX_CHANNELS) {
        ep_channel_registry[ep_channel_count++] = chan;
    }
    pthread_mutex_unlock(&ep_channel_registry_mutex);
}

/* Channel scanning implementations — called by GC mark via function pointers.
   These are defined here (after EpChannel) so they can access struct fields. */
static void ep_gc_mark_object(void* ptr);     /* forward decl */
static void ep_gc_mark_object_minor(void* ptr); /* forward decl */

static void ep_gc_scan_channels_major_impl(void) {
    pthread_mutex_lock(&ep_channel_registry_mutex);
    for (int c = 0; c < ep_channel_count; c++) {
        EpChannel* chan = ep_channel_registry[c];
        if (!chan || chan->size <= 0) continue;
        ep_mutex_lock(&chan->mutex);
        for (long long j = 0; j < chan->size; j++) {
            long long idx = (chan->head + j) % chan->capacity;
            long long val = chan->data[idx];
            if (val != 0) ep_gc_mark_object((void*)val);
        }
        ep_mutex_unlock(&chan->mutex);
    }
    pthread_mutex_unlock(&ep_channel_registry_mutex);
}

static void ep_gc_scan_channels_minor_impl(void) {
    pthread_mutex_lock(&ep_channel_registry_mutex);
    for (int c = 0; c < ep_channel_count; c++) {
        EpChannel* chan = ep_channel_registry[c];
        if (!chan || chan->size <= 0) continue;
        ep_mutex_lock(&chan->mutex);
        for (long long j = 0; j < chan->size; j++) {
            long long idx = (chan->head + j) % chan->capacity;
            long long val = chan->data[idx];
            if (val != 0) ep_gc_mark_object_minor((void*)val);
        }
        ep_mutex_unlock(&chan->mutex);
    }
    pthread_mutex_unlock(&ep_channel_registry_mutex);
}

long long create_channel(void) {
    EpChannel* chan = malloc(sizeof(EpChannel));
    if (!chan) return 0;
    chan->capacity = 1024;
    chan->data = malloc(chan->capacity * sizeof(long long));
    chan->head = 0;
    chan->tail = 0;
    chan->size = 0;
    ep_mutex_init(&chan->mutex);
    ep_cond_init(&chan->cond_recv);
    ep_cond_init(&chan->cond_send);
    ep_register_channel(chan);
    return (long long)chan;
}

long long send_channel(long long chan_ptr, long long value) {
    EpChannel* chan = (EpChannel*)chan_ptr;
    if (!chan) return 0;
    /* Suppress GC during channel operations. The blocking condvar wait
       can interleave with GC mark/sweep on another thread, causing
       use-after-free when the GC sweeps objects that are live on a
       thread currently blocked in send/receive. Channel buffers contain
       raw long long values (not GC-tracked pointers), so suppressing
       GC here is safe. */
    int gc_was_enabled = ep_gc_enabled;
    ep_gc_enabled = 0;
    ep_mutex_lock(&chan->mutex);
    while (chan->size >= chan->capacity) {
        ep_cond_wait(&chan->cond_send, &chan->mutex);
    }
    chan->data[chan->tail] = value;
    chan->tail = (chan->tail + 1) % chan->capacity;
    chan->size += 1;
    ep_cond_signal(&chan->cond_recv);
    ep_mutex_unlock(&chan->mutex);
    ep_gc_enabled = gc_was_enabled;
    return value;
}

long long receive_channel(long long chan_ptr) {
    EpChannel* chan = (EpChannel*)chan_ptr;
    if (!chan) return 0;
    /* Suppress GC during channel receive — same rationale as send_channel */
    int gc_was_enabled = ep_gc_enabled;
    ep_gc_enabled = 0;
    ep_mutex_lock(&chan->mutex);
    while (chan->size <= 0) {
        ep_cond_wait(&chan->cond_recv, &chan->mutex);
    }
    long long value = chan->data[chan->head];
    chan->head = (chan->head + 1) % chan->capacity;
    chan->size -= 1;
    ep_cond_signal(&chan->cond_send);
    ep_mutex_unlock(&chan->mutex);
    ep_gc_enabled = gc_was_enabled;
    return value;
}

// Non-blocking receive — returns 1 if data was available, 0 if channel empty
long long channel_try_recv(long long chan_ptr, long long out_ptr) {
    EpChannel* chan = (EpChannel*)chan_ptr;
    if (!chan) return 0;
    ep_mutex_lock(&chan->mutex);
    if (chan->size <= 0) {
        ep_mutex_unlock(&chan->mutex);
        return 0;
    }
    long long value = chan->data[chan->head];
    chan->head = (chan->head + 1) % chan->capacity;
    chan->size -= 1;
    ep_cond_signal(&chan->cond_send);
    ep_mutex_unlock(&chan->mutex);
    if (out_ptr) {
        *((long long*)out_ptr) = value;
    }
    return 1;
}

// Check if channel has data without consuming it
long long channel_has_data(long long chan_ptr) {
    EpChannel* chan = (EpChannel*)chan_ptr;
    if (!chan) return 0;
    ep_mutex_lock(&chan->mutex);
    int has = (chan->size > 0) ? 1 : 0;
    ep_mutex_unlock(&chan->mutex);
    return has;
}

// Select: wait for any of N channels to have data, with timeout in ms
// channels_list is a list of channel pointers
// Returns index (0-based) of first ready channel, or -1 on timeout
long long channel_select(long long channels_list, long long timeout_ms) {
    EpList* list = (EpList*)channels_list;
    if (!list || list->length == 0) return -1;
    
#ifdef _WIN32
    ULONGLONG start_tick = GetTickCount64();
#else
    struct timespec start, now;
    clock_gettime(CLOCK_MONOTONIC, &start);
#endif
    
    while (1) {
        // Poll all channels
        for (long long i = 0; i < list->length; i++) {
            EpChannel* chan = (EpChannel*)list->data[i];
            if (chan) {
                ep_mutex_lock(&chan->mutex);
                if (chan->size > 0) {
                    ep_mutex_unlock(&chan->mutex);
                    return i;
                }
                ep_mutex_unlock(&chan->mutex);
            }
        }
        
        // Check timeout
        if (timeout_ms >= 0) {
#ifdef _WIN32
            ULONGLONG now_tick = GetTickCount64();
            long long elapsed = (long long)(now_tick - start_tick);
#else
            clock_gettime(CLOCK_MONOTONIC, &now);
            long long elapsed = (now.tv_sec - start.tv_sec) * 1000 + (now.tv_nsec - start.tv_nsec) / 1000000;
#endif
            if (elapsed >= timeout_ms) return -1;
        }
        
        // Brief sleep to avoid busy-wait
#ifdef _WIN32
        Sleep(1);
#else
        usleep(1000); // 1ms
#endif
    }
}

#ifdef __wasm__
long long ep_net_connect(const char* host, long long port) {
    (void)host; (void)port;
    return -1;
}

long long ep_net_listen(long long port) {
    (void)port;
    return -1;
}

long long ep_net_accept(long long server_fd) {
    (void)server_fd;
    return -1;
}

long long ep_net_send(long long fd, const char* data) {
    (void)fd; (void)data;
    return 0;
}

char* ep_net_recv(long long fd, long long max_len) {
    (void)fd; (void)max_len;
    char* empty = malloc(1);
    if (empty) empty[0] = '\0';
    return empty;
}

long long ep_net_close(long long fd) {
    (void)fd;
    return -1;
}

long long ep_sleep_ms(long long ms) {
    struct timespec ts;
    ts.tv_sec = ms / 1000;
    ts.tv_nsec = (ms % 1000) * 1000000;
    nanosleep(&ts, NULL);
    return 0;
}

long long ep_system(long long cmd) {
    (void)cmd;
    return -1;
}

long long ep_play_sound(long long path) {
    (void)path;
    return -1;
}

long long ep_dlopen(long long path) {
    (void)path;
    return 0;
}

long long ep_dlsym(long long handle, long long name) {
    (void)handle; (void)name;
    return 0;
}

long long ep_dlclose(long long handle) {
    (void)handle;
    return 0;
}
#else
long long ep_net_connect(const char* host, long long port) {
    int sockfd = socket(AF_INET, SOCK_STREAM, 0);
    if (sockfd < 0) return -1;
    struct hostent* server = gethostbyname(host);
    if (!server) {
#ifdef _WIN32
        closesocket(sockfd);
#else
        close(sockfd);
#endif
        return -1;
    }
    struct sockaddr_in serv_addr;
    memset(&serv_addr, 0, sizeof(serv_addr));
    serv_addr.sin_family = AF_INET;
    memcpy(&serv_addr.sin_addr.s_addr, server->h_addr_list[0], server->h_length);
    serv_addr.sin_port = htons(port);
#ifdef _WIN32
    if (connect(sockfd, (struct sockaddr*)&serv_addr, sizeof(serv_addr)) < 0) {
        closesocket(sockfd);
        return -1;
    }
#else
    // Bounded connect: an unreachable peer must not block ~75s on the OS SYN
    // timeout (this stalled node startup). Non-blocking connect + 5s select, then
    // restore blocking mode for the rest of the session.
    int _ep_flags = fcntl(sockfd, F_GETFL, 0);
    fcntl(sockfd, F_SETFL, _ep_flags | O_NONBLOCK);
    int _ep_cr = connect(sockfd, (struct sockaddr*)&serv_addr, sizeof(serv_addr));
    if (_ep_cr < 0) {
        if (errno != EINPROGRESS) { close(sockfd); return -1; }
        fd_set _ep_wset; FD_ZERO(&_ep_wset); FD_SET(sockfd, &_ep_wset);
        struct timeval _ep_tv; _ep_tv.tv_sec = 5; _ep_tv.tv_usec = 0;
        int _ep_sel = select(sockfd + 1, NULL, &_ep_wset, NULL, &_ep_tv);
        if (_ep_sel <= 0) { close(sockfd); return -1; } // timeout or error
        int _ep_so_err = 0; socklen_t _ep_slen = sizeof(_ep_so_err);
        if (getsockopt(sockfd, SOL_SOCKET, SO_ERROR, &_ep_so_err, &_ep_slen) < 0 || _ep_so_err != 0) {
            close(sockfd);
            return -1;
        }
    }
    fcntl(sockfd, F_SETFL, _ep_flags);
#endif
    return sockfd;
}

long long ep_net_listen(long long port) {
    int sockfd = socket(AF_INET, SOCK_STREAM, 0);
    if (sockfd < 0) return -1;
    int opt = 1;
    setsockopt(sockfd, SOL_SOCKET, SO_REUSEADDR, (const char*)&opt, sizeof(opt));
    struct sockaddr_in serv_addr;
    memset(&serv_addr, 0, sizeof(serv_addr));
    serv_addr.sin_family = AF_INET;
    serv_addr.sin_addr.s_addr = INADDR_ANY;
    serv_addr.sin_port = htons(port);
    if (bind(sockfd, (struct sockaddr*)&serv_addr, sizeof(serv_addr)) < 0) {
#ifdef _WIN32
        closesocket(sockfd);
#else
        close(sockfd);
#endif
        return -1;
    }
    if (listen(sockfd, 10) < 0) {
#ifdef _WIN32
        closesocket(sockfd);
#else
        close(sockfd);
#endif
        return -1;
    }
    return sockfd;
}

long long ep_net_accept(long long server_fd) {
    struct sockaddr_in cli_addr;
    socklen_t clilen = sizeof(cli_addr);
    int newsockfd = accept((int)server_fd, (struct sockaddr*)&cli_addr, &clilen);
    if (newsockfd >= 0) {
        /* Bound how long a single recv/send may block so a slow or silent
           client cannot pin a handler thread forever (slowloris). */
        struct timeval tv;
        tv.tv_sec = 30;
        tv.tv_usec = 0;
        setsockopt(newsockfd, SOL_SOCKET, SO_RCVTIMEO, (const char*)&tv, sizeof(tv));
        setsockopt(newsockfd, SOL_SOCKET, SO_SNDTIMEO, (const char*)&tv, sizeof(tv));
    }
    return newsockfd;
}

long long ep_net_send(long long fd, const char* data) {
    if (!data) return 0;
    /* send() may write fewer bytes than requested (partial write under load/
       backpressure). A single send() therefore silently truncated large IPC
       responses, cutting agent replies mid-stream. Loop until all bytes are sent. */
    size_t total = strlen(data);
    size_t off = 0;
    while (off < total) {
        ssize_t n = send((int)fd, data + off, total - off, 0);
        if (n <= 0) break;
        off += (size_t)n;
    }
    return (long long)off;
}

char* ep_net_recv(long long fd, long long max_len) {
    char* buf = malloc(max_len + 1);
    if (!buf) {
        char* empty = malloc(1);
        if (empty) empty[0] = '\0';
        return empty;
    }
#ifdef _WIN32
    int n = recv((int)fd, buf, (int)max_len, 0);
#else
    ssize_t n = recv((int)fd, buf, max_len, 0);
#endif
    if (n < 0) n = 0;
    buf[n] = '\0';
    return buf;
}

long long ep_net_close(long long fd) {
#ifdef _WIN32
    return closesocket((int)fd);
#else
    return close((int)fd);
#endif
}

long long ep_sleep_ms(long long ms) {
#ifdef _WIN32
    Sleep((DWORD)ms);
#else
    usleep((useconds_t)(ms * 1000));
#endif
    return 0;
}

long long ep_system(long long cmd) {
    return (long long)system((const char*)cmd);
}

long long ep_play_sound(long long path) {
    char cmd[512];
    snprintf(cmd, sizeof(cmd), "afplay '%s' &", (const char*)path);
    return (long long)system(cmd);
}

/* ========== Dynamic Library Loading (FFI) ========== */
#ifndef _WIN32
#include <dlfcn.h>
#endif

long long ep_dlopen(long long path) {
#ifdef _WIN32
    HMODULE h = LoadLibraryA((const char*)path);
    return (long long)h;
#else
    const char* p = (const char*)path;
    void* handle = dlopen(p, RTLD_LAZY);
    return (long long)handle;
#endif
}

long long ep_dlsym(long long handle, long long name) {
#ifdef _WIN32
    FARPROC sym = GetProcAddress((HMODULE)handle, (const char*)name);
    return (long long)sym;
#else
    void* sym = dlsym((void*)handle, (const char*)name);
    return (long long)sym;
#endif
}

long long ep_dlclose(long long handle) {
#ifdef _WIN32
    return (long long)FreeLibrary((HMODULE)handle);
#else
    return (long long)dlclose((void*)handle);
#endif
}
#endif

/* Call a function pointer with 0..6 arguments.
   These are type-punned through long long — the C calling convention
   makes this work for integer and pointer arguments. */
typedef long long (*ep_fn0)(void);
typedef long long (*ep_fn1)(long long);
typedef long long (*ep_fn2)(long long, long long);
typedef long long (*ep_fn3)(long long, long long, long long);
typedef long long (*ep_fn4)(long long, long long, long long, long long);
typedef long long (*ep_fn5)(long long, long long, long long, long long, long long);
typedef long long (*ep_fn6)(long long, long long, long long, long long, long long, long long);
typedef long long (*ep_fn7)(long long, long long, long long, long long, long long, long long, long long);
typedef long long (*ep_fn8)(long long, long long, long long, long long, long long, long long, long long, long long);
typedef long long (*ep_fn9)(long long, long long, long long, long long, long long, long long, long long, long long, long long);
typedef long long (*ep_fn10)(long long, long long, long long, long long, long long, long long, long long, long long, long long, long long);

long long ep_dlcall0(long long fptr) {
    return ((ep_fn0)fptr)();
}
long long ep_dlcall1(long long fptr, long long a0) {
    return ((ep_fn1)fptr)(a0);
}
long long ep_dlcall2(long long fptr, long long a0, long long a1) {
    return ((ep_fn2)fptr)(a0, a1);
}
long long ep_dlcall3(long long fptr, long long a0, long long a1, long long a2) {
    return ((ep_fn3)fptr)(a0, a1, a2);
}
long long ep_dlcall4(long long fptr, long long a0, long long a1, long long a2, long long a3) {
    return ((ep_fn4)fptr)(a0, a1, a2, a3);
}
long long ep_dlcall5(long long fptr, long long a0, long long a1, long long a2, long long a3, long long a4) {
    return ((ep_fn5)fptr)(a0, a1, a2, a3, a4);
}
long long ep_dlcall6(long long fptr, long long a0, long long a1, long long a2, long long a3, long long a4, long long a5) {
    return ((ep_fn6)fptr)(a0, a1, a2, a3, a4, a5);
}
long long ep_dlcall7(long long fptr, long long a0, long long a1, long long a2, long long a3, long long a4, long long a5, long long a6) {
    return ((ep_fn7)fptr)(a0, a1, a2, a3, a4, a5, a6);
}
long long ep_dlcall8(long long fptr, long long a0, long long a1, long long a2, long long a3, long long a4, long long a5, long long a6, long long a7) {
    return ((ep_fn8)fptr)(a0, a1, a2, a3, a4, a5, a6, a7);
}
long long ep_dlcall9(long long fptr, long long a0, long long a1, long long a2, long long a3, long long a4, long long a5, long long a6, long long a7, long long a8) {
    return ((ep_fn9)fptr)(a0, a1, a2, a3, a4, a5, a6, a7, a8);
}
long long ep_dlcall10(long long fptr, long long a0, long long a1, long long a2, long long a3, long long a4, long long a5, long long a6, long long a7, long long a8, long long a9) {
    return ((ep_fn10)fptr)(a0, a1, a2, a3, a4, a5, a6, a7, a8, a9);
}

/* ========== Float FFI: ep_dlcall_f* ========== */
/* For calling C functions that accept/return double values.
   Arguments are passed as long long (bit-punned doubles).
   Return value is a double bit-punned back to long long.
   Use ep_double_to_bits() / ep_bits_to_double() to convert. */

typedef union { long long i; double f; } ep_float_bits;

static inline double ep_ll_to_double(long long v) {
    ep_float_bits u; u.i = v; return u.f;
}
static inline long long ep_double_to_ll(double v) {
    ep_float_bits u; u.f = v; return u.i;
}

/* Convert between ErnosPlain float representation and raw bits */
long long ep_double_to_bits(long long float_val) {
    /* float_val is already an EP Float stored as long long bits */
    return float_val;
}
long long ep_bits_to_double(long long bits) {
    return bits;
}

/* Float function pointer typedefs */
typedef double (*ep_ff0)(void);
typedef double (*ep_ff1)(double);
typedef double (*ep_ff2)(double, double);
typedef double (*ep_ff3)(double, double, double);
typedef double (*ep_ff4)(double, double, double, double);
typedef double (*ep_ff5)(double, double, double, double, double);
typedef double (*ep_ff6)(double, double, double, double, double, double);

/* Call functions that take doubles and return double */
long long ep_dlcall_f0(long long fptr) {
    return ep_double_to_ll(((ep_ff0)fptr)());
}
long long ep_dlcall_f1(long long fptr, long long a0) {
    return ep_double_to_ll(((ep_ff1)fptr)(ep_ll_to_double(a0)));
}
long long ep_dlcall_f2(long long fptr, long long a0, long long a1) {
    return ep_double_to_ll(((ep_ff2)fptr)(ep_ll_to_double(a0), ep_ll_to_double(a1)));
}
long long ep_dlcall_f3(long long fptr, long long a0, long long a1, long long a2) {
    return ep_double_to_ll(((ep_ff3)fptr)(ep_ll_to_double(a0), ep_ll_to_double(a1), ep_ll_to_double(a2)));
}
long long ep_dlcall_f4(long long fptr, long long a0, long long a1, long long a2, long long a3) {
    return ep_double_to_ll(((ep_ff4)fptr)(ep_ll_to_double(a0), ep_ll_to_double(a1), ep_ll_to_double(a2), ep_ll_to_double(a3)));
}
long long ep_dlcall_f5(long long fptr, long long a0, long long a1, long long a2, long long a3, long long a4) {
    return ep_double_to_ll(((ep_ff5)fptr)(ep_ll_to_double(a0), ep_ll_to_double(a1), ep_ll_to_double(a2), ep_ll_to_double(a3), ep_ll_to_double(a4)));
}
long long ep_dlcall_f6(long long fptr, long long a0, long long a1, long long a2, long long a3, long long a4, long long a5) {
    return ep_double_to_ll(((ep_ff6)fptr)(ep_ll_to_double(a0), ep_ll_to_double(a1), ep_ll_to_double(a2), ep_ll_to_double(a3), ep_ll_to_double(a4), ep_ll_to_double(a5)));
}

/* Variants that take doubles but return int (for comparison functions etc.) */
typedef long long (*ep_fdi1)(double);
typedef long long (*ep_fdi2)(double, double);
typedef long long (*ep_fdi3)(double, double, double);

long long ep_dlcall_fd1(long long fptr, long long a0) {
    return ((ep_fdi1)fptr)(ep_ll_to_double(a0));
}
long long ep_dlcall_fd2(long long fptr, long long a0, long long a1) {
    return ((ep_fdi2)fptr)(ep_ll_to_double(a0), ep_ll_to_double(a1));
}
long long ep_dlcall_fd3(long long fptr, long long a0, long long a1, long long a2) {
    return ((ep_fdi3)fptr)(ep_ll_to_double(a0), ep_ll_to_double(a1), ep_ll_to_double(a2));
}
/* ========== End Float FFI ========== */
/* ========== End Dynamic Library Loading ========== */

unsigned long hash_string(const char* str) {
    unsigned long hash = 5381;
    int c;
    while ((c = *str++)) {
        hash = ((hash << 5) + hash) + c;
    }
    return hash;
}

typedef struct {
    char* key;
    long long value;
    int used;
} EpMapEntry;

typedef struct {
    EpMapEntry* entries;
    long long capacity;
    long long size;
} EpMap;

/* Map value traversal for GC — walks all entries and marks values.
   Called by ep_gc_mark_object() via function pointer. */
static void ep_gc_mark_map_values_impl(void* ptr) {
    EpMap* map = (EpMap*)ptr;
    if (!map || !map->entries) return;
    for (long long i = 0; i < map->capacity; i++) {
        if (map->entries[i].used && map->entries[i].value != 0) {
            ep_gc_mark_object((void*)map->entries[i].value);
        }
        /* Also mark keys if they are heap strings */
        if (map->entries[i].used && map->entries[i].key != NULL) {
            ep_gc_mark_object((void*)map->entries[i].key);
        }
    }
}

static void ep_gc_mark_map_values_minor_impl(void* ptr) {
    EpMap* map = (EpMap*)ptr;
    if (!map || !map->entries) return;
    for (long long i = 0; i < map->capacity; i++) {
        if (map->entries[i].used && map->entries[i].value != 0) {
            ep_gc_mark_object_minor((void*)map->entries[i].value);
        }
        if (map->entries[i].used && map->entries[i].key != NULL) {
            ep_gc_mark_object_minor((void*)map->entries[i].key);
        }
    }
}

long long create_map(void) {
    EpMap* map = malloc(sizeof(EpMap));
    if (!map) return 0;
    map->capacity = 16;
    map->size = 0;
    map->entries = calloc(map->capacity, sizeof(EpMapEntry));
    if (!map->entries) {
        free(map);
        return 0;
    }
    ep_gc_register(map, EP_OBJ_MAP);
    return (long long)map;
}

static void map_resize(EpMap* map, long long new_capacity) {
    EpMapEntry* old_entries = map->entries;
    long long old_capacity = map->capacity;
    map->capacity = new_capacity;
    map->entries = calloc(new_capacity, sizeof(EpMapEntry));
    map->size = 0;
    for (long long i = 0; i < old_capacity; i++) {
        if (old_entries[i].used && old_entries[i].key != NULL) {
            char* key = old_entries[i].key;
            long long value = old_entries[i].value;
            unsigned long h = hash_string(key) % new_capacity;
            while (map->entries[h].used) {
                h = (h + 1) % new_capacity;
            }
            map->entries[h].key = key;
            map->entries[h].value = value;
            map->entries[h].used = 1;
            map->size++;
        }
    }
    free(old_entries);
}

/* Convert a key value to a string — handles both string pointers and integers */
static const char* ep_map_key_str(long long key_val, char* buf, int bufsize) {
    if (key_val == 0) { buf[0] = '0'; buf[1] = '\0'; return buf; }
    /* Check if value is in plausible pointer range for a string */
    if (key_val > 0x100000) {
        const char* p = (const char*)(void*)key_val;
        unsigned char first = (unsigned char)*p;
        if ((first >= 0x20 && first < 0x7F) || first >= 0xC0 || first == 0) {
            return p; /* valid string pointer */
        }
    }
    snprintf(buf, bufsize, "%lld", key_val);
    return buf;
}

long long map_insert(long long map_ptr, long long key_val, long long value) {
    if (EP_BADPTR(map_ptr)) return 0;
    EpMap* map = (EpMap*)map_ptr;
    char keybuf[32];
    const char* key = ep_map_key_str(key_val, keybuf, sizeof(keybuf));
    if (!map) return 0;
    if (map->size * 2 >= map->capacity) {
        map_resize(map, map->capacity * 2);
    }
    unsigned long h = hash_string(key) % map->capacity;
    while (map->entries[h].used) {
        if (strcmp(map->entries[h].key, key) == 0) {
            map->entries[h].value = value;
            ep_gc_write_barrier((void*)map_ptr, value);
            return value;
        }
        h = (h + 1) % map->capacity;
    }
    map->entries[h].key = strdup(key);
    map->entries[h].value = value;
    map->entries[h].used = 1;
    map->size++;
    ep_gc_write_barrier((void*)map_ptr, value);
    return value;
}

long long map_get_val(long long map_ptr, long long key_val) {
    if (EP_BADPTR(map_ptr)) return 0;
    EpMap* map = (EpMap*)map_ptr;
    char keybuf[32];
    const char* key = ep_map_key_str(key_val, keybuf, sizeof(keybuf));
    if (!map) return 0;
    unsigned long h = hash_string(key) % map->capacity;
    long long start_h = h;
    while (map->entries[h].used) {
        if (map->entries[h].key && strcmp(map->entries[h].key, key) == 0) {
            return map->entries[h].value;
        }
        h = (h + 1) % map->capacity;
        if (h == start_h) break;
    }
    return 0;
}

/* map_set_str: store a string value (strdup'd copy) under a string key */
long long map_set_str(long long map_ptr, long long key_val, long long str_val) {
    /* Store the string pointer as a long long value — same as map_insert */
    return map_insert(map_ptr, key_val, str_val);
}

/* map_get_str: retrieve a string value from a map (returns char* as long long) */
long long map_get_str(long long map_ptr, long long key_val) {
    /* Same as map_get_val — the stored long long IS a char* pointer */
    return map_get_val(map_ptr, key_val);
}

long long map_contains(long long map_ptr, long long key_val) {
    if (EP_BADPTR(map_ptr)) return 0;
    EpMap* map = (EpMap*)map_ptr;
    char keybuf[32];
    const char* key = ep_map_key_str(key_val, keybuf, sizeof(keybuf));
    if (!map) return 0;
    unsigned long h = hash_string(key) % map->capacity;
    long long start_h = h;
    while (map->entries[h].used) {
        if (map->entries[h].key && strcmp(map->entries[h].key, key) == 0) {
            return 1;
        }
        h = (h + 1) % map->capacity;
        if (h == start_h) break;
    }
    return 0;
}

long long map_delete(long long map_ptr, long long key_val) {
    if (EP_BADPTR(map_ptr)) return 0;
    EpMap* map = (EpMap*)map_ptr;
    char keybuf[32];
    const char* key = ep_map_key_str(key_val, keybuf, sizeof(keybuf));
    if (!map) return 0;
    unsigned long h = hash_string(key) % map->capacity;
    long long start_h = h;
    while (map->entries[h].used) {
        if (map->entries[h].key && strcmp(map->entries[h].key, key) == 0) {
            free(map->entries[h].key);
            map->entries[h].key = NULL;
            map->entries[h].value = 0;
            map->entries[h].used = 0;
            map->size--;
            long long next_h = (h + 1) % map->capacity;
            while (map->entries[next_h].used) {
                char* k = map->entries[next_h].key;
                long long v = map->entries[next_h].value;
                map->entries[next_h].key = NULL;
                map->entries[next_h].value = 0;
                map->entries[next_h].used = 0;
                map->size--;
                map_insert(map_ptr, (long long)k, v);
                free(k);
                next_h = (next_h + 1) % map->capacity;
            }
            return 1;
        }
        h = (h + 1) % map->capacity;
        if (h == start_h) break;
    }
    return 0;
}

long long map_keys(long long map_ptr) {
    EpMap* map = (EpMap*)map_ptr;
    if (!map) return (long long)create_list();
    long long list = create_list();
    for (long long i = 0; i < map->capacity; i++) {
        if (map->entries[i].used && map->entries[i].key) {
            append_list(list, (long long)strdup(map->entries[i].key));
        }
    }
    return list;
}

long long map_values(long long map_ptr) {
    EpMap* map = (EpMap*)map_ptr;
    if (!map) return (long long)create_list();
    long long list = create_list();
    for (long long i = 0; i < map->capacity; i++) {
        if (map->entries[i].used && map->entries[i].key) {
            append_list(list, map->entries[i].value);
        }
    }
    return list;
}

long long map_size(long long map_ptr) {
    EpMap* map = (EpMap*)map_ptr;
    if (!map) return 0;
    return map->size;
}

long long free_map(long long map_ptr) {
    EpMap* map = (EpMap*)map_ptr;
    if (!map) return 0;
    /* Skip if already freed (idempotent) */
    if (!ep_gc_find(map)) return 0;
    ep_gc_unregister(map);
    for (long long i = 0; i < map->capacity; i++) {
        if (map->entries[i].used && map->entries[i].key != NULL) {
            free(map->entries[i].key);
        }
    }
    free(map->entries);
    free(map);
    return 0;
}

typedef struct {
    long long* data;
    long long capacity;
    long long head;
    long long tail;
    long long size;
} EpDeque;

long long create_deque(void) {
    EpDeque* dq = malloc(sizeof(EpDeque));
    if (!dq) return 0;
    dq->capacity = 16;
    dq->size = 0;
    dq->head = 0;
    dq->tail = 0;
    dq->data = malloc(dq->capacity * sizeof(long long));
    if (!dq->data) {
        free(dq);
        return 0;
    }
    return (long long)dq;
}

static void deque_resize(EpDeque* dq, long long new_capacity) {
    long long* new_data = malloc(new_capacity * sizeof(long long));
    for (long long i = 0; i < dq->size; i++) {
        new_data[i] = dq->data[(dq->head + i) % dq->capacity];
    }
    free(dq->data);
    dq->data = new_data;
    dq->capacity = new_capacity;
    dq->head = 0;
    dq->tail = dq->size;
}

long long deque_push_back(long long dq_ptr, long long value) {
    EpDeque* dq = (EpDeque*)dq_ptr;
    if (!dq) return 0;
    if (dq->size >= dq->capacity) {
        deque_resize(dq, dq->capacity * 2);
    }
    dq->data[dq->tail] = value;
    dq->tail = (dq->tail + 1) % dq->capacity;
    dq->size++;
    return value;
}

long long deque_push_front(long long dq_ptr, long long value) {
    EpDeque* dq = (EpDeque*)dq_ptr;
    if (!dq) return 0;
    if (dq->size >= dq->capacity) {
        deque_resize(dq, dq->capacity * 2);
    }
    dq->head = (dq->head - 1 + dq->capacity) % dq->capacity;
    dq->data[dq->head] = value;
    dq->size++;
    return value;
}

long long deque_pop_back(long long dq_ptr) {
    EpDeque* dq = (EpDeque*)dq_ptr;
    if (!dq || dq->size == 0) return 0;
    dq->tail = (dq->tail - 1 + dq->capacity) % dq->capacity;
    long long value = dq->data[dq->tail];
    dq->size--;
    return value;
}

long long deque_pop_front(long long dq_ptr) {
    EpDeque* dq = (EpDeque*)dq_ptr;
    if (!dq || dq->size == 0) return 0;
    long long value = dq->data[dq->head];
    dq->head = (dq->head + 1) % dq->capacity;
    dq->size--;
    return value;
}

long long deque_length(long long dq_ptr) {
    EpDeque* dq = (EpDeque*)dq_ptr;
    if (!dq) return 0;
    return dq->size;
}

long long free_deque(long long dq_ptr) {
    EpDeque* dq = (EpDeque*)dq_ptr;
    if (!dq) return 0;
    free(dq->data);
    free(dq);
    return 0;
}

/* Filesystem Operations */
#include <dirent.h>
#include <sys/stat.h>

long long fs_scan_dir(long long path_val) {
    const char* path = (const char*)path_val;
    long long list_ptr = create_list();
    if (!path) return list_ptr;
    DIR* d = opendir(path);
    if (!d) return list_ptr;
    struct dirent* dir;
    while ((dir = readdir(d)) != NULL) {
        if (strcmp(dir->d_name, ".") == 0 || strcmp(dir->d_name, "..") == 0) {
            continue;
        }
        char* name = strdup(dir->d_name);
        append_list(list_ptr, (long long)name);
    }
    closedir(d);
    return list_ptr;
}

long long fs_copy_file(long long src_val, long long dest_val) {
    const char* src = (const char*)src_val;
    const char* dest = (const char*)dest_val;
    if (!src || !dest) return 0;
    FILE* f_src = fopen(src, "rb");
    if (!f_src) return 0;
    FILE* f_dest = fopen(dest, "wb");
    if (!f_dest) {
        fclose(f_src);
        return 0;
    }
    char buf[4096];
    size_t n;
    while ((n = fread(buf, 1, sizeof(buf), f_src)) > 0) {
        fwrite(buf, 1, n, f_dest);
    }
    fclose(f_src);
    fclose(f_dest);
    return 1;
}

long long fs_delete_file(long long path_val) {
    const char* path = (const char*)path_val;
    if (!path) return 0;
    return remove(path) == 0 ? 1 : 0;
}

long long fs_move_file(long long src_val, long long dest_val) {
    const char* src = (const char*)src_val;
    const char* dest = (const char*)dest_val;
    if (!src || !dest) return 0;
    return rename(src, dest) == 0 ? 1 : 0;
}

long long fs_exists(long long path_val) {
    const char* path = (const char*)path_val;
    if (!path) return 0;
    struct stat st;
    return stat(path, &st) == 0 ? 1 : 0;
}

long long fs_is_dir(long long path_val) {
    const char* path = (const char*)path_val;
    if (!path) return 0;
    struct stat st;
    if (stat(path, &st) != 0) return 0;
    return S_ISDIR(st.st_mode) ? 1 : 0;
}

long long fs_is_file(long long path_val) {
    const char* path = (const char*)path_val;
    if (!path) return 0;
    struct stat st;
    if (stat(path, &st) != 0) return 0;
    return S_ISREG(st.st_mode) ? 1 : 0;
}

long long fs_get_size(long long path_val) {
    const char* path = (const char*)path_val;
    if (!path) return 0;
    struct stat st;
    if (stat(path, &st) != 0) return 0;
    return (long long)st.st_size;
}

/* HTTP Client */
#ifdef __wasm__
long long ep_http_request(long long method_val, long long url_val, long long headers_val, long long body_val) {
    (void)method_val; (void)url_val; (void)headers_val; (void)body_val;
    return (long long)strdup("Error: HTTP request is not supported on WebAssembly");
}
#else
long long ep_http_request(long long method_val, long long url_val, long long headers_val, long long body_val) {
    const char* method = (const char*)method_val;
    const char* url = (const char*)url_val;
    const char* headers = (const char*)headers_val;
    const char* body = (const char*)body_val;
    if (!method || !url) return (long long)strdup("");
    if (strncmp(url, "http://", 7) != 0) {
        return (long long)strdup("Error: only http:// protocol supported");
    }
    const char* host_start = url + 7;
    const char* path_start = strchr(host_start, '/');
    char host[256];
    char path[1024];
    if (path_start) {
        size_t host_len = path_start - host_start;
        if (host_len >= sizeof(host)) host_len = sizeof(host) - 1;
        strncpy(host, host_start, host_len);
        host[host_len] = '\0';
        strncpy(path, path_start, sizeof(path) - 1);
        path[sizeof(path) - 1] = '\0';
    } else {
        strncpy(host, host_start, sizeof(host) - 1);
        host[sizeof(host) - 1] = '\0';
        strcpy(path, "/");
    }
    int port = 80;
    char* colon = strchr(host, ':');
    if (colon) {
        *colon = '\0';
        port = atoi(colon + 1);
    }
    int sockfd = socket(AF_INET, SOCK_STREAM, 0);
    if (sockfd < 0) return (long long)strdup("Error: socket creation failed");
    struct hostent* server = gethostbyname(host);
    if (!server) {
        close(sockfd);
        return (long long)strdup("Error: host resolution failed");
    }
    struct sockaddr_in serv_addr;
    memset(&serv_addr, 0, sizeof(serv_addr));
    serv_addr.sin_family = AF_INET;
    memcpy(&serv_addr.sin_addr.s_addr, server->h_addr_list[0], server->h_length);
    serv_addr.sin_port = htons(port);
    if (connect(sockfd, (struct sockaddr*)&serv_addr, sizeof(serv_addr)) < 0) {
        close(sockfd);
        return (long long)strdup("Error: connection failed");
    }
    char req[4096];
    size_t body_len = body ? strlen(body) : 0;
    int req_len = snprintf(req, sizeof(req),
        "%s %s HTTP/1.1\r\n"
        "Host: %s\r\n"
        "Content-Length: %zu\r\n"
        "Connection: close\r\n"
        "%s%s"
        "\r\n",
        method, path, host, body_len, headers ? headers : "", (headers && strlen(headers) > 0 && headers[strlen(headers)-1] != '\n') ? "\r\n" : "");
    if (send(sockfd, req, req_len, 0) < 0) {
        close(sockfd);
        return (long long)strdup("Error: send failed");
    }
    if (body_len > 0) {
        if (send(sockfd, body, body_len, 0) < 0) {
            close(sockfd);
            return (long long)strdup("Error: send body failed");
        }
    }
    size_t resp_cap = 4096;
    size_t resp_len = 0;
    char* resp = malloc(resp_cap);
    if (!resp) {
        close(sockfd);
        return (long long)strdup("");
    }
    char recv_buf[4096];
    ssize_t n;
    while ((n = recv(sockfd, recv_buf, sizeof(recv_buf), 0)) > 0) {
        if (resp_len + n >= resp_cap) {
            resp_cap *= 2;
            char* new_resp = realloc(resp, resp_cap);
            if (!new_resp) {
                free(resp);
                close(sockfd);
                return (long long)strdup("Error: memory allocation failed");
            }
            resp = new_resp;
        }
        memcpy(resp + resp_len, recv_buf, n);
        resp_len += n;
    }
    resp[resp_len] = '\0';
    close(sockfd);
    // Strip HTTP headers — return only the body after \r\n\r\n
    char* http_body = strstr(resp, "\r\n\r\n");
    if (http_body) {
        http_body += 4;
        char* result = strdup(http_body);
        free(resp);
        return (long long)result;
    }
    return (long long)resp;
}
#endif

#define ROTRIGHT(word,bits) (((word) >> (bits)) | ((word) << (32-(bits))))
#define CH(x,y,z) (((x) & (y)) ^ (~(x) & (z)))
#define MAJ(x,y,z) (((x) & (y)) ^ ((x) & (z)) ^ ((y) & (z)))
#define EP0(x) (ROTRIGHT(x,2) ^ ROTRIGHT(x,13) ^ ROTRIGHT(x,22))
#define EP1(x) (ROTRIGHT(x,6) ^ ROTRIGHT(x,11) ^ ROTRIGHT(x,25))
#define SIG0(x) (ROTRIGHT(x,7) ^ ROTRIGHT(x,18) ^ ((x) >> 3))
#define SIG1(x) (ROTRIGHT(x,17) ^ ROTRIGHT(x,19) ^ ((x) >> 10))

typedef struct {
    unsigned char data[64];
    unsigned int datalen;
    unsigned long long bitlen;
    unsigned int state[8];
} EP_SHA256_CTX;

static const unsigned int sha256_k[64] = {
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2
};

void ep_sha256_transform(EP_SHA256_CTX *ctx, const unsigned char *data) {
    unsigned int a, b, c, d, e, f, g, h, i, j, t1, t2, m[64];
    for (i = 0, j = 0; i < 16; ++i, j += 4)
        m[i] = (data[j] << 24) | (data[j + 1] << 16) | (data[j + 2] << 8) | (data[j + 3]);
    for ( ; i < 64; ++i)
        m[i] = SIG1(m[i - 2]) + m[i - 7] + SIG0(m[i - 15]) + m[i - 16];
    a = ctx->state[0]; b = ctx->state[1]; c = ctx->state[2]; d = ctx->state[3];
    e = ctx->state[4]; f = ctx->state[5]; g = ctx->state[6]; h = ctx->state[7];
    for (i = 0; i < 64; ++i) {
        t1 = h + EP1(e) + CH(e,f,g) + sha256_k[i] + m[i];
        t2 = EP0(a) + MAJ(a,b,c);
        h = g; g = f; f = e; e = d + t1; d = c; c = b; b = a; a = t1 + t2;
    }
    ctx->state[0] += a; ctx->state[1] += b; ctx->state[2] += c; ctx->state[3] += d;
    ctx->state[4] += e; ctx->state[5] += f; ctx->state[6] += g; ctx->state[7] += h;
}

void ep_sha256_init(EP_SHA256_CTX *ctx) {
    ctx->datalen = 0; ctx->bitlen = 0;
    ctx->state[0] = 0x6a09e667; ctx->state[1] = 0xbb67ae85; ctx->state[2] = 0x3c6ef372; ctx->state[3] = 0xa54ff53a;
    ctx->state[4] = 0x510e527f; ctx->state[5] = 0x9b05688c; ctx->state[6] = 0x1f83d9ab; ctx->state[7] = 0x5be0cd19;
}

void ep_sha256_update(EP_SHA256_CTX *ctx, const unsigned char *data, size_t len) {
    for (size_t i = 0; i < len; ++i) {
        ctx->data[ctx->datalen] = data[i];
        ctx->datalen++;
        if (ctx->datalen == 64) {
            ep_sha256_transform(ctx, ctx->data);
            ctx->bitlen += 512;
            ctx->datalen = 0;
        }
    }
}

void ep_sha256_final(EP_SHA256_CTX *ctx, unsigned char *hash) {
    unsigned int i = ctx->datalen;
    if (ctx->datalen < 56) {
        ctx->data[i++] = 0x80;
        while (i < 56) ctx->data[i++] = 0x00;
    } else {
        ctx->data[i++] = 0x80;
        while (i < 64) ctx->data[i++] = 0x00;
        ep_sha256_transform(ctx, ctx->data);
        memset(ctx->data, 0, 56);
    }
    ctx->bitlen += ctx->datalen * 8;
    ctx->data[63] = ctx->bitlen; ctx->data[62] = ctx->bitlen >> 8;
    ctx->data[61] = ctx->bitlen >> 16; ctx->data[60] = ctx->bitlen >> 24;
    ctx->data[59] = ctx->bitlen >> 32; ctx->data[58] = ctx->bitlen >> 40;
    ctx->data[57] = ctx->bitlen >> 48; ctx->data[56] = ctx->bitlen >> 56;
    ep_sha256_transform(ctx, ctx->data);
    for (i = 0; i < 4; ++i) {
        hash[i]      = (ctx->state[0] >> (24 - i * 8)) & 0x000000ff;
        hash[i + 4]  = (ctx->state[1] >> (24 - i * 8)) & 0x000000ff;
        hash[i + 8]  = (ctx->state[2] >> (24 - i * 8)) & 0x000000ff;
        hash[i + 12] = (ctx->state[3] >> (24 - i * 8)) & 0x000000ff;
        hash[i + 16] = (ctx->state[4] >> (24 - i * 8)) & 0x000000ff;
        hash[i + 20] = (ctx->state[5] >> (24 - i * 8)) & 0x000000ff;
        hash[i + 24] = (ctx->state[6] >> (24 - i * 8)) & 0x000000ff;
        hash[i + 28] = (ctx->state[7] >> (24 - i * 8)) & 0x000000ff;
    }
}

char* ep_sha256(const char* s) {
    if (!s) s = "";
    EP_SHA256_CTX ctx;
    ep_sha256_init(&ctx);
    ep_sha256_update(&ctx, (const unsigned char*)s, strlen(s));
    unsigned char hash[32];
    ep_sha256_final(&ctx, hash);
    char* result = malloc(65);
    if (result) {
        for (int i = 0; i < 32; i++) {
            snprintf(result + (i * 2), 3, "%02x", hash[i]);
        }
        result[64] = '\0';
    }
    return result;
}

/* RFC 2104 HMAC-SHA256. Operates on raw bytes with explicit lengths (binary
   safe), so keys/messages containing NUL bytes hash correctly. Returns a
   malloc'd 64-char lowercase hex string. */
long long ep_hmac_sha256(long long key_ptr, long long key_len, long long msg_ptr, long long msg_len) {
    const unsigned char* key = (const unsigned char*)key_ptr;
    const unsigned char* msg = (const unsigned char*)msg_ptr;
    size_t klen = (size_t)key_len;
    size_t mlen = (size_t)msg_len;

    unsigned char k0[64];
    memset(k0, 0, sizeof(k0));
    if (klen > 64) {
        /* Keys longer than the block size are replaced by their hash. */
        EP_SHA256_CTX kc;
        ep_sha256_init(&kc);
        ep_sha256_update(&kc, key ? key : (const unsigned char*)"", klen);
        unsigned char kh[32];
        ep_sha256_final(&kc, kh);
        memcpy(k0, kh, 32);
    } else if (key) {
        memcpy(k0, key, klen);
    }

    unsigned char ipad[64], opad[64];
    for (int i = 0; i < 64; i++) {
        ipad[i] = k0[i] ^ 0x36;
        opad[i] = k0[i] ^ 0x5c;
    }

    /* inner = H((K0 ^ ipad) || message) */
    EP_SHA256_CTX ic;
    ep_sha256_init(&ic);
    ep_sha256_update(&ic, ipad, 64);
    if (msg && mlen) ep_sha256_update(&ic, msg, mlen);
    unsigned char inner[32];
    ep_sha256_final(&ic, inner);

    /* mac = H((K0 ^ opad) || inner) */
    EP_SHA256_CTX oc;
    ep_sha256_init(&oc);
    ep_sha256_update(&oc, opad, 64);
    ep_sha256_update(&oc, inner, 32);
    unsigned char mac[32];
    ep_sha256_final(&oc, mac);

    char* out = (char*)malloc(65);
    if (out) {
        for (int i = 0; i < 32; i++) {
            snprintf(out + (i * 2), 3, "%02x", mac[i]);
        }
        out[64] = '\0';
    }
    return (long long)out;
}

typedef struct {
    unsigned int count[2];
    unsigned int state[4];
    unsigned char buffer[64];
} EP_MD5_CTX;

#define F(x,y,z) (((x) & (y)) | (~(x) & (z)))
#define G(x,y,z) (((x) & (z)) | ((y) & ~(z)))
#define H(x,y,z) ((x) ^ (y) ^ (z))
#define I(x,y,z) ((y) ^ ((x) | ~(z)))
#define ROTATE_LEFT(x,n) (((x) << (n)) | ((x) >> (32-(n))))

#define FF(a,b,c,d,x,s,ac) { \
    (a) += F((b),(c),(d)) + (x) + (ac); \
    (a) = ROTATE_LEFT((a),(s)); \
    (a) += (b); \
}
#define GG(a,b,c,d,x,s,ac) { \
    (a) += G((b),(c),(d)) + (x) + (ac); \
    (a) = ROTATE_LEFT((a),(s)); \
    (a) += (b); \
}
#define HH(a,b,c,d,x,s,ac) { \
    (a) += H((b),(c),(d)) + (x) + (ac); \
    (a) = ROTATE_LEFT((a),(s)); \
    (a) += (b); \
}
#define II(a,b,c,d,x,s,ac) { \
    (a) += I((b),(c),(d)) + (x) + (ac); \
    (a) = ROTATE_LEFT((a),(s)); \
    (a) += (b); \
}

void ep_md5_init(EP_MD5_CTX *ctx) {
    ctx->count[0] = ctx->count[1] = 0;
    ctx->state[0] = 0x67452301;
    ctx->state[1] = 0xefcdab89;
    ctx->state[2] = 0x98badcfe;
    ctx->state[3] = 0x10325476;
}

void ep_md5_transform(unsigned int state[4], const unsigned char block[64]) {
    unsigned int a = state[0], b = state[1], c = state[2], d = state[3], x[16];
    for (int i = 0, j = 0; i < 16; i++, j += 4)
        x[i] = (block[j]) | (block[j+1] << 8) | (block[j+2] << 16) | (block[j+3] << 24);

    FF(a, b, c, d, x[0], 7, 0xd76aa478); FF(d, a, b, c, x[1], 12, 0xe8c7b756); FF(c, d, a, b, x[2], 17, 0x242070db); FF(b, c, d, a, x[3], 22, 0xc1bdceee);
    FF(a, b, c, d, x[4], 7, 0xf57c0faf); FF(d, a, b, c, x[5], 12, 0x4787c62a); FF(c, d, a, b, x[6], 17, 0xa8304613); FF(b, c, d, a, x[7], 22, 0xfd469501);
    FF(a, b, c, d, x[8], 7, 0x698098d8); FF(d, a, b, c, x[9], 12, 0x8b44f7af); FF(c, d, a, b, x[10], 17, 0xffff5bb1); FF(b, c, d, a, x[11], 22, 0x895cd7be);
    FF(a, b, c, d, x[12], 7, 0x6b901122); FF(d, a, b, c, x[13], 12, 0xfd987193); FF(c, d, a, b, x[14], 17, 0xa679438e); FF(b, c, d, a, x[15], 22, 0x49b40821);

    GG(a, b, c, d, x[1], 5, 0xf61e2562); GG(d, a, b, c, x[6], 9, 0xc040b340); GG(c, d, a, b, x[11], 14, 0x265e5a51); GG(b, c, d, a, x[0], 20, 0xe9b6c7aa);
    GG(a, b, c, d, x[5], 5, 0xd62f105d); GG(d, a, b, c, x[10], 9, 0x02441453); GG(c, d, a, b, x[15], 14, 0xd8a1e681); GG(b, c, d, a, x[4], 20, 0xe7d3fbc8);
    GG(a, b, c, d, x[9], 5, 0x21e1cde6); GG(d, a, b, c, x[14], 9, 0xc33707d6); GG(c, d, a, b, x[3], 14, 0xf4d50d87); GG(b, c, d, a, x[8], 20, 0x455a14ed);
    GG(a, b, c, d, x[13], 5, 0xa9e3e905); GG(d, a, b, c, x[2], 9, 0xfcefa3f8); GG(c, d, a, b, x[7], 14, 0x676f02d9); GG(b, c, d, a, x[12], 20, 0x8d2a4c8a);

    HH(a, b, c, d, x[5], 4, 0xfffa3942); HH(d, a, b, c, x[8], 11, 0x8771f681); HH(c, d, a, b, x[11], 16, 0x6d9d6122); HH(b, c, d, a, x[14], 23, 0xfde5380c);
    HH(a, b, c, d, x[1], 4, 0xa4beea44); HH(d, a, b, c, x[4], 11, 0x4bdecfa9); HH(c, d, a, b, x[7], 16, 0xf6bb4b60); HH(b, c, d, a, x[10], 23, 0xbebfbc70);
    HH(a, b, c, d, x[13], 4, 0x289b7ec6); HH(d, a, b, c, x[0], 11, 0xeaa127fa); HH(c, d, a, b, x[3], 16, 0xd4ef3085); HH(b, c, d, a, x[6], 23, 0x04881d05);
    HH(a, b, c, d, x[9], 4, 0xd9d4d039); HH(d, a, b, c, x[12], 11, 0xe6db99e5); HH(c, d, a, b, x[15], 16, 0x1fa27cf8); HH(b, c, d, a, x[2], 23, 0xc4ac5665);

    II(a, b, c, d, x[0], 6, 0xf4292244); II(d, a, b, c, x[7], 10, 0x432aff97); II(c, d, a, b, x[14], 15, 0xab9423a7); II(b, c, d, a, x[5], 21, 0xfc93a039);
    II(a, b, c, d, x[12], 6, 0x655b59c3); II(d, a, b, c, x[3], 10, 0x8f0ccc92); II(c, d, a, b, x[10], 15, 0xffeff47d); II(b, c, d, a, x[1], 21, 0x85845dd1);
    II(a, b, c, d, x[8], 6, 0x6fa87e4f); II(d, a, b, c, x[15], 10, 0xfe2ce6e0); II(c, d, a, b, x[6], 15, 0xa3014314); II(b, c, d, a, x[13], 21, 0x4e0811a1);
    II(a, b, c, d, x[4], 6, 0xf7537e82); II(d, a, b, c, x[11], 10, 0xbd3af235); II(c, d, a, b, x[2], 15, 0x2ad7d2bb); II(b, c, d, a, x[9], 21, 0xeb86d391);

    state[0] += a; state[1] += b; state[2] += c; state[3] += d;
}

void ep_md5_update(EP_MD5_CTX *ctx, const unsigned char *input, size_t input_len) {
    unsigned int i = 0, index = (ctx->count[0] >> 3) & 0x3F, part_len = 64 - index;
    ctx->count[0] += input_len << 3;
    if (ctx->count[0] < (input_len << 3)) ctx->count[1]++;
    ctx->count[1] += input_len >> 29;
    if (input_len >= part_len) {
        memcpy(&ctx->buffer[index], input, part_len);
        ep_md5_transform(ctx->state, ctx->buffer);
        for (i = part_len; i + 63 < input_len; i += 64)
            ep_md5_transform(ctx->state, &input[i]);
        index = 0;
    }
    memcpy(&ctx->buffer[index], &input[i], input_len - i);
}

void ep_md5_final(EP_MD5_CTX *ctx, unsigned char digest[16]) {
    unsigned char bits[8];
    bits[0] = ctx->count[0]; bits[1] = ctx->count[0] >> 8; bits[2] = ctx->count[0] >> 16; bits[3] = ctx->count[0] >> 24;
    bits[4] = ctx->count[1]; bits[5] = ctx->count[1] >> 8; bits[6] = ctx->count[1] >> 16; bits[7] = ctx->count[1] >> 24;
    unsigned int index = (ctx->count[0] >> 3) & 0x3F, pad_len = (index < 56) ? (56 - index) : (120 - index);
    unsigned char padding[64];
    memset(padding, 0, 64); padding[0] = 0x80;
    ep_md5_update(ctx, padding, pad_len);
    ep_md5_update(ctx, bits, 8);
    for (int i = 0; i < 4; i++) {
        digest[i*4]     = ctx->state[i];
        digest[i*4 + 1] = ctx->state[i] >> 8;
        digest[i*4 + 2] = ctx->state[i] >> 16;
        digest[i*4 + 3] = ctx->state[i] >> 24;
    }
}

char* ep_md5(const char* s) {
    if (!s) s = "";
    EP_MD5_CTX ctx;
    ep_md5_init(&ctx);
    ep_md5_update(&ctx, (const unsigned char*)s, strlen(s));
    unsigned char hash[16];
    ep_md5_final(&ctx, hash);
    char* result = malloc(33);
    if (result) {
        for (int i = 0; i < 16; i++) {
            snprintf(result + (i * 2), 3, "%02x", hash[i]);
        }
        result[32] = '\0';
    }
    return result;
}

char* read_file_content(const char* filepath) {
    char mode[3];
    mode[0] = 'r';
    mode[1] = 'b';
    mode[2] = '\0';
    FILE* f = fopen(filepath, mode);
    if (!f) {
        char* empty = malloc(1);
        if (empty) empty[0] = '\0';
        ep_gc_register(empty, EP_OBJ_STRING);
        return empty;
    }
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    char* buf = malloc(size + 1);
    if (!buf) {
        fclose(f);
        char* empty = malloc(1);
        if (empty) empty[0] = '\0';
        ep_gc_register(empty, EP_OBJ_STRING);
        return empty;
    }
    size_t read_bytes = fread(buf, 1, size, f);
    buf[read_bytes] = '\0';
    fclose(f);
    ep_gc_register(buf, EP_OBJ_STRING);
    return buf;
}

long long string_length(const char* s) {
    if (!s) return 0;
    return strlen(s);
}

long long get_character(const char* s, long long index) {
    if (!s) return 0;
    long long len = strlen(s);
    if (index < 0 || index >= len) return 0;
    return (unsigned char)s[index];
}

long long create_list(void) {
    EpList* list = malloc(sizeof(EpList));
    if (!list) return 0;
    list->capacity = 4;
    list->length = 0;
    list->data = malloc(list->capacity * sizeof(long long));
    ep_gc_register(list, EP_OBJ_LIST);
    return (long long)list;
}

long long get_list_data_ptr(long long list_ptr) {
    if (EP_BADPTR(list_ptr)) return 0;
    EpList* list = (EpList*)list_ptr;
    if (!list) return 0;
    return (long long)list->data;
}

long long append_list(long long list_ptr, long long value) {
    if (EP_BADPTR(list_ptr)) return 0;
    EpList* list = (EpList*)list_ptr;
    if (!list) return 0;
    if (list->length >= list->capacity) {
        list->capacity *= 2;
        list->data = realloc(list->data, list->capacity * sizeof(long long));
    }
    list->data[list->length] = value;
    list->length += 1;
    ep_gc_write_barrier((void*)list_ptr, value);
    return value;
}

long long get_list(long long list_ptr, long long index) {
    if (EP_BADPTR(list_ptr)) return 0;
    EpList* list = (EpList*)list_ptr;
    if (index < 0 || index >= list->length) return 0;
    return list->data[index];
}

long long set_list(long long list_ptr, long long index, long long value) {
    if (EP_BADPTR(list_ptr)) return 0;
    EpList* list = (EpList*)list_ptr;
    if (index < 0 || index >= list->length) return 0;
    list->data[index] = value;
    ep_gc_write_barrier((void*)list_ptr, value);
    return value;
}

long long length_list(long long list_ptr) {
    if (EP_BADPTR(list_ptr)) return 0;
    EpList* list = (EpList*)list_ptr;
    return list->length;
}

long long free_list(long long list_ptr) {
    EpList* list = (EpList*)list_ptr;
    if (!list) return 0;
    /* Skip if already freed (idempotent) */
    if (!ep_gc_find(list)) return 0;
    ep_gc_unregister(list);
    free(list->data);
    free(list);
    return 0;
}

static int sqlite_list_callback(void* arg, int argc, char** argv, char** col_names) {
    EpList* rows = (EpList*)arg;
    EpList* row = (EpList*)create_list();
    for (int i = 0; i < argc; i++) {
        char* val = argv[i] ? strdup(argv[i]) : strdup("");
        append_list((long long)row, (long long)val);
    }
    append_list((long long)rows, (long long)row);
    return 0;
}

long long sqlite_get_callback_ptr(long long dummy) {
    return (long long)sqlite_list_callback;
}

/* SQLite type-safe wrappers — marshal between int and long long */
#ifdef EP_HAS_SQLITE
typedef struct sqlite3 sqlite3;
int sqlite3_open(const char*, sqlite3**);
int sqlite3_close(sqlite3*);
int sqlite3_exec(sqlite3*, const char*, int(*)(void*,int,char**,char**), void*, char**);

long long ep_sqlite3_open(long long filename, long long db_ptr) {
    sqlite3* db = NULL;
    int rc = sqlite3_open((const char*)filename, &db);
    if (rc == 0 && db_ptr != 0) {
        *((long long*)db_ptr) = (long long)db;
    }
    return (long long)rc;
}

long long ep_sqlite3_close(long long db) {
    return (long long)sqlite3_close((sqlite3*)db);
}

long long ep_sqlite3_exec(long long db, long long sql, long long callback, long long cb_arg, long long errmsg_ptr) {
    return (long long)sqlite3_exec((sqlite3*)db, (const char*)sql,
        (int(*)(void*,int,char**,char**))(callback),
        (void*)cb_arg, (char**)errmsg_ptr);
}

/* Prepared-statement API for parameterized queries (defeats SQL injection). */
typedef struct sqlite3_stmt sqlite3_stmt;
int sqlite3_prepare_v2(sqlite3*, const char*, int, sqlite3_stmt**, const char**);
int sqlite3_bind_text(sqlite3_stmt*, int, const char*, int, void(*)(void*));
int sqlite3_bind_int64(sqlite3_stmt*, int, long long);
int sqlite3_step(sqlite3_stmt*);
int sqlite3_column_count(sqlite3_stmt*);
const unsigned char* sqlite3_column_text(sqlite3_stmt*, int);
long long sqlite3_column_int64(sqlite3_stmt*, int);
int sqlite3_finalize(sqlite3_stmt*);

long long ep_sqlite3_prepare_v2(long long db, long long sql) {
    sqlite3_stmt* stmt = NULL;
    int rc = sqlite3_prepare_v2((sqlite3*)db, (const char*)sql, -1, &stmt, NULL);
    if (rc != 0) return 0;
    return (long long)stmt;
}

long long ep_sqlite3_bind_text(long long stmt, long long idx, long long value) {
    /* SQLITE_TRANSIENT ((void*)-1): sqlite copies the bound string. The value is
       a bound parameter, never concatenated into SQL — this is the safe path. */
    return (long long)sqlite3_bind_text((sqlite3_stmt*)stmt, (int)idx,
        (const char*)value, -1, (void(*)(void*))(intptr_t)-1);
}

long long ep_sqlite3_bind_int(long long stmt, long long idx, long long value) {
    return (long long)sqlite3_bind_int64((sqlite3_stmt*)stmt, (int)idx, value);
}

long long ep_sqlite3_step(long long stmt) {
    return (long long)sqlite3_step((sqlite3_stmt*)stmt);
}

long long ep_sqlite3_column_count(long long stmt) {
    return (long long)sqlite3_column_count((sqlite3_stmt*)stmt);
}

long long ep_sqlite3_column_text(long long stmt, long long col) {
    const unsigned char* t = sqlite3_column_text((sqlite3_stmt*)stmt, (int)col);
    if (!t) return (long long)strdup("");
    return (long long)strdup((const char*)t);
}

long long ep_sqlite3_column_int(long long stmt, long long col) {
    return sqlite3_column_int64((sqlite3_stmt*)stmt, (int)col);
}

long long ep_sqlite3_finalize(long long stmt) {
    return (long long)sqlite3_finalize((sqlite3_stmt*)stmt);
}
#endif /* EP_HAS_SQLITE */

int ep_argc = 0;
char** ep_argv = NULL;

void init_ep_args(int argc, char** argv) {
    ep_argc = argc;
    ep_argv = argv;
    ep_gc_register_thread((void*)&argc);
    /* Wire up channel scanning for GC (defined after EpChannel struct) */
    ep_gc_scan_channels_major = ep_gc_scan_channels_major_impl;
    ep_gc_scan_channels_minor = ep_gc_scan_channels_minor_impl;
    /* Wire up map value traversal for GC (defined after EpMap struct) */
    ep_gc_mark_map_values = ep_gc_mark_map_values_impl;
    ep_gc_mark_map_values_minor = ep_gc_mark_map_values_minor_impl;
}

long long get_argument_count(void) {
    return ep_argc;
}

const char* get_argument(long long index) {
    if (index < 0 || index >= ep_argc) {
        char* empty = malloc(1);
        if (empty) empty[0] = '\0';
        return empty;
    }
    return ep_argv[index];
}

long long write_file_content(const char* filepath, const char* content) {
    char mode[3];
    mode[0] = 'w';
    mode[1] = 'b';
    mode[2] = '\0';
    FILE* f = fopen(filepath, mode);
    if (!f) return 0;
    size_t len = strlen(content);
    size_t written = fwrite(content, 1, len, f);
    fclose(f);
    return written == len ? 1 : 0;
}

long long run_command(const char* command) {
    if (!command) return -1;
    return system(command);
}

char* substring(const char* s, long long start, long long len) {
    if (!s) {
        char* empty = malloc(1);
        if (empty) empty[0] = '\0';
        ep_gc_register(empty, EP_OBJ_STRING);
        return empty;
    }
    long long total_len = strlen(s);
    if (start < 0 || start >= total_len || len <= 0) {
        char* empty = malloc(1);
        if (empty) empty[0] = '\0';
        ep_gc_register(empty, EP_OBJ_STRING);
        return empty;
    }
    if (start + len > total_len) {
        len = total_len - start;
    }
    char* sub = malloc(len + 1);
    if (!sub) {
        char* empty = malloc(1);
        if (empty) empty[0] = '\0';
        ep_gc_register(empty, EP_OBJ_STRING);
        return empty;
    }
    strncpy(sub, s + start, len);
    sub[len] = '\0';
    ep_gc_register(sub, EP_OBJ_STRING);
    return sub;
}

char* string_from_list(long long list_ptr) {
    EpList* list = (EpList*)list_ptr;
    if (!list) {
        char* empty = malloc(1);
        if (empty) empty[0] = '\0';
        ep_gc_register(empty, EP_OBJ_STRING);
        return empty;
    }
    char* s = malloc(list->length + 1);
    if (!s) {
        char* empty = malloc(1);
        if (empty) empty[0] = '\0';
        ep_gc_register(empty, EP_OBJ_STRING);
        return empty;
    }
    for (long long i = 0; i < list->length; i++) {
        s[i] = (char)list->data[i];
    }
    s[list->length] = '\0';
    ep_gc_register(s, EP_OBJ_STRING);
    return s;
}

// Inverse of string_from_list: convert a string to a list of its byte values in
// a single O(n) pass (one strlen + one copy). This lets callers iterate a string
// in O(n) total via O(1) get_list, instead of O(n) get_character per index
// (which is O(n^2) over the whole string).
long long string_to_list(const char* s) {
    EpList* list = malloc(sizeof(EpList));
    if (!list) return 0;
    long long len = s ? (long long)strlen(s) : 0;
    list->capacity = len > 0 ? len : 4;
    list->length = len;
    list->data = malloc(list->capacity * sizeof(long long));
    if (!list->data) {
        list->capacity = 0;
        list->length = 0;
        ep_gc_register(list, EP_OBJ_LIST);
        return (long long)list;
    }
    for (long long i = 0; i < len; i++) {
        list->data[i] = (unsigned char)s[i];
    }
    ep_gc_register(list, EP_OBJ_LIST);
    return (long long)list;
}

long long pop_list(long long list_ptr) {
    if (EP_BADPTR(list_ptr)) return 0;
    EpList* list = (EpList*)list_ptr;
    if (!list || list->length <= 0) return 0;
    list->length -= 1;
    return list->data[list->length];
}

long long remove_list(long long list_ptr, long long index) {
    if (EP_BADPTR(list_ptr)) return 0;
    EpList* list = (EpList*)list_ptr;
    if (!list || index < 0 || index >= list->length) return 0;
    long long removed = list->data[index];
    for (long long i = index; i < list->length - 1; i++) {
        list->data[i] = list->data[i + 1];
    }
    list->length -= 1;
    return removed;
}

long long display_string(const char* s) {
    if (s) puts(s);
    return 0;
}

/* ========== File System Runtime ========== */
#include <sys/stat.h>
#ifdef _WIN32
  #include <io.h>
  #include <direct.h>
  #define mkdir(p, m) _mkdir(p)
  #define rmdir _rmdir
  #define getcwd _getcwd
  #define popen _popen
  #define pclose _pclose
  #define getpid _getpid
  #define setenv(k, v, o) _putenv_s(k, v)
  /* Minimal dirent polyfill for Windows */
  #include <windows.h>
  typedef struct { char d_name[260]; } ep_dirent;
  typedef struct { HANDLE hFind; WIN32_FIND_DATAA data; int first; } EP_DIR;
  static EP_DIR* ep_opendir(const char* p) {
      EP_DIR* d = (EP_DIR*)malloc(sizeof(EP_DIR));
      char buf[270]; snprintf(buf, sizeof(buf), "%s\\*", p);
      d->hFind = FindFirstFileA(buf, &d->data);
      d->first = 1;
      return (d->hFind == INVALID_HANDLE_VALUE) ? (free(d), (EP_DIR*)NULL) : d;
  }
  static ep_dirent* ep_readdir(EP_DIR* d) {
      static ep_dirent ent;
      if (d->first) { d->first = 0; strcpy(ent.d_name, d->data.cFileName); return &ent; }
      if (!FindNextFileA(d->hFind, &d->data)) return NULL;
      strcpy(ent.d_name, d->data.cFileName); return &ent;
  }
  static void ep_closedir(EP_DIR* d) { FindClose(d->hFind); free(d); }
  #define DIR EP_DIR
  #define dirent ep_dirent
  #define opendir ep_opendir
  #define readdir ep_readdir
  #define closedir ep_closedir
#else
  #include <dirent.h>
  #include <unistd.h>
#endif

long long ep_read_file(long long path_ptr) {
    const char* path = (const char*)path_ptr;
    FILE* f = fopen(path, "rb");
    if (!f) return (long long)"";
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    char* buf = (char*)malloc(size + 1);
    fread(buf, 1, size, f);
    buf[size] = '\0';
    fclose(f);
    return (long long)buf;
}

long long ep_write_file(long long path_ptr, long long content_ptr) {
    const char* path = (const char*)path_ptr;
    const char* content = (const char*)content_ptr;
    FILE* f = fopen(path, "wb");
    if (!f) return 0;
    fputs(content, f);
    fclose(f);
    return 1;
}

long long ep_append_file(long long path_ptr, long long content_ptr) {
    const char* path = (const char*)path_ptr;
    const char* content = (const char*)content_ptr;
    FILE* f = fopen(path, "ab");
    if (!f) return 0;
    fputs(content, f);
    fclose(f);
    return 1;
}

long long ep_file_exists(long long path_ptr) {
    const char* path = (const char*)path_ptr;
    struct stat st;
    return stat(path, &st) == 0 ? 1 : 0;
}

long long ep_is_directory(long long path_ptr) {
    const char* path = (const char*)path_ptr;
    struct stat st;
    if (stat(path, &st) != 0) return 0;
    return S_ISDIR(st.st_mode) ? 1 : 0;
}

long long ep_file_size(long long path_ptr) {
    const char* path = (const char*)path_ptr;
    struct stat st;
    if (stat(path, &st) != 0) return -1;
    return (long long)st.st_size;
}

long long ep_list_directory(long long path_ptr) {
    const char* path = (const char*)path_ptr;
    DIR* dir = opendir(path);
    if (!dir) return (long long)create_list();
    long long list = create_list();
    struct dirent* entry;
    while ((entry = readdir(dir)) != NULL) {
        if (entry->d_name[0] == '.' && (entry->d_name[1] == '\0' || 
            (entry->d_name[1] == '.' && entry->d_name[2] == '\0'))) continue;
        char* name = strdup(entry->d_name);
        append_list(list, (long long)name);
    }
    closedir(dir);
    return list;
}

long long ep_create_directory(long long path_ptr) {
    const char* path = (const char*)path_ptr;
    return mkdir(path, 0755) == 0 ? 1 : 0;
}

long long ep_remove_file(long long path_ptr) {
    const char* path = (const char*)path_ptr;
    return remove(path) == 0 ? 1 : 0;
}

long long ep_remove_directory(long long path_ptr) {
    const char* path = (const char*)path_ptr;
    return rmdir(path) == 0 ? 1 : 0;
}

long long ep_rename_file(long long old_ptr, long long new_ptr) {
    return rename((const char*)old_ptr, (const char*)new_ptr) == 0 ? 1 : 0;
}

long long ep_copy_file(long long src_ptr, long long dst_ptr) {
    const char* src = (const char*)src_ptr;
    const char* dst = (const char*)dst_ptr;
    FILE* fin = fopen(src, "rb");
    if (!fin) return 0;
    FILE* fout = fopen(dst, "wb");
    if (!fout) { fclose(fin); return 0; }
    char buf[8192];
    size_t n;
    while ((n = fread(buf, 1, sizeof(buf), fin)) > 0) {
        fwrite(buf, 1, n, fout);
    }
    fclose(fin);
    fclose(fout);
    return 1;
}

/* ========== Date/Time Runtime ========== */
#include <time.h>
#include <sys/time.h>

long long ep_time_now_ms(void) {
    struct timeval tv;
    gettimeofday(&tv, NULL);
    return (long long)tv.tv_sec * 1000LL + (long long)tv.tv_usec / 1000LL;
}

long long ep_time_now_sec(void) {
    return (long long)time(NULL);
}


long long ep_time_year(long long ts) {
    time_t t = (time_t)ts;
    struct tm* tm = localtime(&t);
    return tm ? tm->tm_year + 1900 : 0;
}

long long ep_time_month(long long ts) {
    time_t t = (time_t)ts;
    struct tm* tm = localtime(&t);
    return tm ? tm->tm_mon + 1 : 0;
}

long long ep_time_day(long long ts) {
    time_t t = (time_t)ts;
    struct tm* tm = localtime(&t);
    return tm ? tm->tm_mday : 0;
}

long long ep_time_hour(long long ts) {
    time_t t = (time_t)ts;
    struct tm* tm = localtime(&t);
    return tm ? tm->tm_hour : 0;
}

long long ep_time_minute(long long ts) {
    time_t t = (time_t)ts;
    struct tm* tm = localtime(&t);
    return tm ? tm->tm_min : 0;
}

long long ep_time_second(long long ts) {
    time_t t = (time_t)ts;
    struct tm* tm = localtime(&t);
    return tm ? tm->tm_sec : 0;
}

long long ep_time_weekday(long long ts) {
    time_t t = (time_t)ts;
    struct tm* tm = localtime(&t);
    return tm ? tm->tm_wday : 0;
}

long long ep_format_time(long long ts, long long fmt_ptr) {
    time_t t = (time_t)ts;
    struct tm* tm = localtime(&t);
    if (!tm) return (long long)"";
    char* buf = (char*)malloc(256);
    strftime(buf, 256, (const char*)fmt_ptr, tm);
    return (long long)buf;
}

/* ========== OS Runtime ========== */

long long ep_getenv(long long name_ptr) {
    const char* val = getenv((const char*)name_ptr);
    return val ? (long long)val : (long long)"";
}

long long ep_setenv(long long name_ptr, long long val_ptr) {
    return setenv((const char*)name_ptr, (const char*)val_ptr, 1) == 0 ? 1 : 0;
}

long long ep_get_cwd(void) {
    char* buf = (char*)malloc(4096);
    if (getcwd(buf, 4096)) return (long long)buf;
    free(buf);
    return (long long)"";
}

long long ep_os_name(void) {
    #if defined(__APPLE__)
    return (long long)"macos";
    #elif defined(__linux__)
    return (long long)"linux";
    #elif defined(_WIN32)
    return (long long)"windows";
    #else
    return (long long)"unknown";
    #endif
}

long long ep_arch_name(void) {
    #if defined(__aarch64__) || defined(__arm64__)
    return (long long)"arm64";
    #elif defined(__x86_64__)
    return (long long)"x86_64";
    #elif defined(__i386__)
    return (long long)"x86";
    #else
    return (long long)"unknown";
    #endif
}

long long ep_exit(long long code) {
    exit((int)code);
    return 0;
}

long long ep_get_pid(void) {
    return (long long)getpid();
}

long long ep_get_home_dir(void) {
    const char* home = getenv("HOME");
    return home ? (long long)home : (long long)"";
}

#ifdef __wasm__
long long ep_run_command(long long cmd_ptr) {
    (void)cmd_ptr;
    return (long long)"Error: running external commands is not supported on WebAssembly";
}
#else
long long ep_run_command(long long cmd_ptr) {
    const char* cmd = (const char*)cmd_ptr;
    FILE* fp = popen(cmd, "r");
    if (!fp) return (long long)"";
    char* result = (char*)malloc(65536);
    size_t total = 0;
    char buf[4096];
    while (fgets(buf, sizeof(buf), fp)) {
        size_t len = strlen(buf);
        memcpy(result + total, buf, len);
        total += len;
    }
    result[total] = '\0';
    pclose(fp);
    return (long long)result;
}
#endif

/* ========== HashMap helpers ========== */

long long ep_hash_string(long long s_ptr) {
    const char* s = (const char*)s_ptr;
    if (!s) return 0;
    unsigned long long hash = 5381;
    int c;
    while ((c = *s++)) {
        hash = ((hash << 5) + hash) + c;
    }
    return (long long)hash;
}

long long ep_str_equals(long long a_ptr, long long b_ptr) {
    if (a_ptr == b_ptr) return 1;
    if (!a_ptr || !b_ptr) return 0;
    /* If either value looks like a small integer (not a valid heap pointer),
       fall back to integer comparison — strcmp would segfault. */
    if ((unsigned long long)a_ptr < 4096ULL || (unsigned long long)b_ptr < 4096ULL) return 0;
    return strcmp((const char*)a_ptr, (const char*)b_ptr) == 0 ? 1 : 0;
}

/* ========== Sync Primitives ========== */

#ifdef _WIN32
long long ep_mutex_create(void) {
    CRITICAL_SECTION* m = (CRITICAL_SECTION*)malloc(sizeof(CRITICAL_SECTION));
    InitializeCriticalSection(m);
    return (long long)m;
}
long long ep_mutex_lock_fn(long long m) {
    EnterCriticalSection((CRITICAL_SECTION*)m);
    return 1;
}
long long ep_mutex_unlock_fn(long long m) {
    LeaveCriticalSection((CRITICAL_SECTION*)m);
    return 1;
}
long long ep_mutex_trylock(long long m) {
    return TryEnterCriticalSection((CRITICAL_SECTION*)m) ? 1 : 0;
}
long long ep_mutex_destroy(long long m) {
    DeleteCriticalSection((CRITICAL_SECTION*)m);
    free((void*)m);
    return 0;
}
#else
long long ep_mutex_create(void) {
    pthread_mutex_t* m = (pthread_mutex_t*)malloc(sizeof(pthread_mutex_t));
    pthread_mutex_init(m, NULL);
    return (long long)m;
}

long long ep_mutex_lock_fn(long long m) {
    return pthread_mutex_lock((pthread_mutex_t*)m) == 0 ? 1 : 0;
}

long long ep_mutex_unlock_fn(long long m) {
    return pthread_mutex_unlock((pthread_mutex_t*)m) == 0 ? 1 : 0;
}

long long ep_mutex_trylock(long long m) {
    return pthread_mutex_trylock((pthread_mutex_t*)m) == 0 ? 1 : 0;
}

long long ep_mutex_destroy(long long m) {
    pthread_mutex_destroy((pthread_mutex_t*)m);
    free((void*)m);
    return 0;
}
#endif

#ifdef _WIN32
long long ep_rwlock_create(void) {
    SRWLOCK* rwl = (SRWLOCK*)malloc(sizeof(SRWLOCK));
    InitializeSRWLock(rwl);
    return (long long)rwl;
}
long long ep_rwlock_read_lock(long long rwl) {
    AcquireSRWLockShared((SRWLOCK*)rwl);
    return 1;
}
long long ep_rwlock_write_lock(long long rwl) {
    AcquireSRWLockExclusive((SRWLOCK*)rwl);
    return 1;
}
long long ep_rwlock_unlock(long long rwl) {
    /* SRWLOCK does not have a single "unlock" — we try exclusive first.
       In practice the caller should know which lock was taken.
       ReleaseSRWLockExclusive on a shared lock is undefined, but
       the runtime guarantees matched lock/unlock pairs. We default
       to releasing the exclusive lock; shared unlock is handled
       by pairing read_lock -> read_unlock if needed later. */
    ReleaseSRWLockExclusive((SRWLOCK*)rwl);
    return 1;
}
long long ep_rwlock_destroy(long long rwl) {
    /* SRWLOCK has no destroy */
    free((void*)rwl);
    return 0;
}
#else
long long ep_rwlock_create(void) {
    pthread_rwlock_t* rwl = (pthread_rwlock_t*)malloc(sizeof(pthread_rwlock_t));
    pthread_rwlock_init(rwl, NULL);
    return (long long)rwl;
}

long long ep_rwlock_read_lock(long long rwl) {
    return pthread_rwlock_rdlock((pthread_rwlock_t*)rwl) == 0 ? 1 : 0;
}

long long ep_rwlock_write_lock(long long rwl) {
    return pthread_rwlock_wrlock((pthread_rwlock_t*)rwl) == 0 ? 1 : 0;
}

long long ep_rwlock_unlock(long long rwl) {
    return pthread_rwlock_unlock((pthread_rwlock_t*)rwl) == 0 ? 1 : 0;
}

long long ep_rwlock_destroy(long long rwl) {
    pthread_rwlock_destroy((pthread_rwlock_t*)rwl);
    free((void*)rwl);
    return 0;
}
#endif

#ifdef _MSC_VER
long long ep_atomic_create(long long initial) {
    volatile long long* a = (volatile long long*)malloc(sizeof(long long));
    InterlockedExchange64(a, initial);
    return (long long)a;
}
long long ep_atomic_load(long long a) {
    return InterlockedCompareExchange64((volatile long long*)a, 0, 0);
}
long long ep_atomic_store(long long a, long long value) {
    InterlockedExchange64((volatile long long*)a, value);
    return value;
}
long long ep_atomic_add(long long a, long long delta) {
    return InterlockedExchangeAdd64((volatile long long*)a, delta);
}
long long ep_atomic_sub(long long a, long long delta) {
    return InterlockedExchangeAdd64((volatile long long*)a, -delta);
}
long long ep_atomic_cas(long long a, long long expected, long long desired) {
    long long old = InterlockedCompareExchange64((volatile long long*)a, desired, expected);
    return (old == expected) ? 1 : 0;
}
#else
long long ep_atomic_create(long long initial) {
    long long* a = (long long*)malloc(sizeof(long long));
    __atomic_store_n(a, initial, __ATOMIC_SEQ_CST);
    return (long long)a;
}

long long ep_atomic_load(long long a) {
    return __atomic_load_n((long long*)a, __ATOMIC_SEQ_CST);
}

long long ep_atomic_store(long long a, long long value) {
    __atomic_store_n((long long*)a, value, __ATOMIC_SEQ_CST);
    return value;
}

long long ep_atomic_add(long long a, long long delta) {
    return __atomic_fetch_add((long long*)a, delta, __ATOMIC_SEQ_CST);
}

long long ep_atomic_sub(long long a, long long delta) {
    return __atomic_fetch_sub((long long*)a, delta, __ATOMIC_SEQ_CST);
}

long long ep_atomic_cas(long long a, long long expected, long long desired) {
    long long exp = expected;
    return __atomic_compare_exchange_n((long long*)a, &exp, desired, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST) ? 1 : 0;
}
#endif

/* Barrier — portable polyfill (macOS lacks pthread_barrier_t) */
typedef struct {
    pthread_mutex_t mutex;
    pthread_cond_t cond;
    unsigned count;
    unsigned target;
    unsigned generation;
} EpBarrier;

long long ep_barrier_create(long long count) {
    EpBarrier* b = (EpBarrier*)malloc(sizeof(EpBarrier));
    pthread_mutex_init(&b->mutex, NULL);
    pthread_cond_init(&b->cond, NULL);
    b->count = 0;
    b->target = (unsigned)count;
    b->generation = 0;
    return (long long)b;
}

long long ep_barrier_wait(long long bp) {
    EpBarrier* b = (EpBarrier*)bp;
    pthread_mutex_lock(&b->mutex);
    unsigned gen = b->generation;
    b->count++;
    if (b->count >= b->target) {
        b->count = 0;
        b->generation++;
        pthread_cond_broadcast(&b->cond);
        pthread_mutex_unlock(&b->mutex);
        return 1; /* serial thread */
    }
    while (gen == b->generation) {
        pthread_cond_wait(&b->cond, &b->mutex);
    }
    pthread_mutex_unlock(&b->mutex);
    return 0;
}

long long ep_barrier_destroy(long long bp) {
    EpBarrier* b = (EpBarrier*)bp;
    pthread_mutex_destroy(&b->mutex);
    pthread_cond_destroy(&b->cond);
    free(b);
    return 0;
}

/* Semaphore via mutex+condvar (portable) */
typedef struct {
    pthread_mutex_t mutex;
    pthread_cond_t cond;
    long long value;
} EpSemaphore;

long long ep_semaphore_create(long long initial) {
    EpSemaphore* s = (EpSemaphore*)malloc(sizeof(EpSemaphore));
    pthread_mutex_init(&s->mutex, NULL);
    pthread_cond_init(&s->cond, NULL);
    s->value = initial;
    return (long long)s;
}

long long ep_semaphore_wait(long long sp) {
    EpSemaphore* s = (EpSemaphore*)sp;
    pthread_mutex_lock(&s->mutex);
    while (s->value <= 0) {
        pthread_cond_wait(&s->cond, &s->mutex);
    }
    s->value--;
    pthread_mutex_unlock(&s->mutex);
    return 1;
}

long long ep_semaphore_post(long long sp) {
    EpSemaphore* s = (EpSemaphore*)sp;
    pthread_mutex_lock(&s->mutex);
    s->value++;
    pthread_cond_signal(&s->cond);
    pthread_mutex_unlock(&s->mutex);
    return 1;
}

long long ep_semaphore_trywait(long long sp) {
    EpSemaphore* s = (EpSemaphore*)sp;
    pthread_mutex_lock(&s->mutex);
    if (s->value > 0) {
        s->value--;
        pthread_mutex_unlock(&s->mutex);
        return 1;
    }
    pthread_mutex_unlock(&s->mutex);
    return 0;
}

long long ep_semaphore_destroy(long long sp) {
    EpSemaphore* s = (EpSemaphore*)sp;
    pthread_mutex_destroy(&s->mutex);
    pthread_cond_destroy(&s->cond);
    free(s);
    return 0;
}

long long ep_condvar_create(void) {
    pthread_cond_t* cv = (pthread_cond_t*)malloc(sizeof(pthread_cond_t));
    pthread_cond_init(cv, NULL);
    return (long long)cv;
}

long long ep_condvar_wait(long long cv, long long m) {
    return pthread_cond_wait((pthread_cond_t*)cv, (pthread_mutex_t*)m) == 0 ? 1 : 0;
}

long long ep_condvar_signal(long long cv) {
    return pthread_cond_signal((pthread_cond_t*)cv) == 0 ? 1 : 0;
}

long long ep_condvar_broadcast(long long cv) {
    return pthread_cond_broadcast((pthread_cond_t*)cv) == 0 ? 1 : 0;
}

long long ep_condvar_destroy(long long cv) {
    pthread_cond_destroy((pthread_cond_t*)cv);
    free((void*)cv);
    return 0;
}

/* ========== Regex (simple stub — delegates to POSIX regex) ========== */
#include <regex.h>

long long ep_regex_match(long long pattern_ptr, long long text_ptr) {
    regex_t regex;
    const char* pattern = (const char*)pattern_ptr;
    const char* text = (const char*)text_ptr;
    int ret = regcomp(&regex, pattern, REG_EXTENDED | REG_NOSUB);
    if (ret) return 0;
    ret = regexec(&regex, text, 0, NULL, 0);
    regfree(&regex);
    return ret == 0 ? 1 : 0;
}

long long ep_regex_find(long long pattern_ptr, long long text_ptr) {
    regex_t regex;
    regmatch_t match;
    const char* pattern = (const char*)pattern_ptr;
    const char* text = (const char*)text_ptr;
    int ret = regcomp(&regex, pattern, REG_EXTENDED);
    if (ret) return (long long)"";
    ret = regexec(&regex, text, 1, &match, 0);
    if (ret != 0) { regfree(&regex); return (long long)""; }
    int len = match.rm_eo - match.rm_so;
    char* result = (char*)malloc(len + 1);
    memcpy(result, text + match.rm_so, len);
    result[len] = '\0';
    regfree(&regex);
    return (long long)result;
}

long long ep_regex_find_all(long long pattern_ptr, long long text_ptr) {
    regex_t regex;
    regmatch_t match;
    const char* pattern = (const char*)pattern_ptr;
    const char* text = (const char*)text_ptr;
    long long list = create_list();
    int ret = regcomp(&regex, pattern, REG_EXTENDED);
    if (ret) return list;
    const char* cursor = text;
    while (regexec(&regex, cursor, 1, &match, 0) == 0) {
        int len = match.rm_eo - match.rm_so;
        char* result = (char*)malloc(len + 1);
        memcpy(result, cursor + match.rm_so, len);
        result[len] = '\0';
        append_list(list, (long long)result);
        cursor += match.rm_eo;
        if (match.rm_eo == 0) break;
    }
    regfree(&regex);
    return list;
}

long long ep_regex_replace(long long pattern_ptr, long long text_ptr, long long repl_ptr) {
    /* Simple single-replacement via regex */
    regex_t regex;
    regmatch_t match;
    const char* pattern = (const char*)pattern_ptr;
    const char* text = (const char*)text_ptr;
    const char* repl = (const char*)repl_ptr;
    int ret = regcomp(&regex, pattern, REG_EXTENDED);
    if (ret) return text_ptr;
    ret = regexec(&regex, text, 1, &match, 0);
    if (ret != 0) { regfree(&regex); return text_ptr; }
    size_t tlen = strlen(text);
    size_t rlen = strlen(repl);
    size_t new_len = tlen - (match.rm_eo - match.rm_so) + rlen;
    char* result = (char*)malloc(new_len + 1);
    memcpy(result, text, match.rm_so);
    memcpy(result + match.rm_so, repl, rlen);
    memcpy(result + match.rm_so + rlen, text + match.rm_eo, tlen - match.rm_eo);
    result[new_len] = '\0';
    regfree(&regex);
    return (long long)result;
}

long long ep_regex_split(long long pattern_ptr, long long text_ptr) {
    long long list = create_list();
    /* Simple split: find matches and split around them */
    regex_t regex;
    regmatch_t match;
    const char* pattern = (const char*)pattern_ptr;
    const char* text = (const char*)text_ptr;
    int ret = regcomp(&regex, pattern, REG_EXTENDED);
    if (ret) {
        append_list(list, text_ptr);
        return list;
    }
    const char* cursor = text;
    while (regexec(&regex, cursor, 1, &match, 0) == 0) {
        int len = match.rm_so;
        char* part = (char*)malloc(len + 1);
        memcpy(part, cursor, len);
        part[len] = '\0';
        append_list(list, (long long)part);
        cursor += match.rm_eo;
        if (match.rm_eo == 0) break;
    }
    char* rest = strdup(cursor);
    append_list(list, (long long)rest);
    regfree(&regex);
    return list;
}

/* ========== Base64 ========== */
static const char b64_table[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

long long ep_base64_encode(long long data_ptr) {
    const unsigned char* data = (const unsigned char*)data_ptr;
    size_t len = strlen((const char*)data);
    size_t out_len = 4 * ((len + 2) / 3);
    char* out = (char*)malloc(out_len + 1);
    size_t i, j = 0;
    for (i = 0; i < len; i += 3) {
        unsigned int n = data[i] << 16;
        if (i + 1 < len) n |= data[i+1] << 8;
        if (i + 2 < len) n |= data[i+2];
        out[j++] = b64_table[(n >> 18) & 63];
        out[j++] = b64_table[(n >> 12) & 63];
        out[j++] = (i + 1 < len) ? b64_table[(n >> 6) & 63] : '=';
        out[j++] = (i + 2 < len) ? b64_table[n & 63] : '=';
    }
    out[j] = '\0';
    return (long long)out;
}

long long ep_uuid_v4(void) {
    char* uuid = (char*)malloc(37);
    unsigned char bytes[16];
    ep_secure_random_bytes(bytes, 16);
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    snprintf(uuid, 37, "%02x%02x%02x%02x-%02x%02x-%02x%02x-%02x%02x-%02x%02x%02x%02x%02x%02x",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11],
        bytes[12], bytes[13], bytes[14], bytes[15]);
    return (long long)uuid;
}

long long file_read(long long path_val) {
    const char* path = (const char*)path_val;
    if (!path) return (long long)strdup("");
    FILE* f = fopen(path, "rb");
    if (!f) return (long long)strdup("");
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    char* buf = malloc(size + 1);
    if (!buf) { fclose(f); return (long long)strdup(""); }
    fread(buf, 1, size, f);
    buf[size] = '\0';
    fclose(f);
    ep_gc_register(buf, EP_OBJ_STRING);
    return (long long)buf;
}

long long file_write(long long path_val, long long content_val) {
    const char* path = (const char*)path_val;
    const char* content = (const char*)content_val;
    if (!path || !content) return 0;
    FILE* f = fopen(path, "wb");
    if (!f) return 0;
    size_t len = strlen(content);
    fwrite(content, 1, len, f);
    fclose(f);
    return 1;
}

long long file_append(long long path_val, long long content_val) {
    const char* path = (const char*)path_val;
    const char* content = (const char*)content_val;
    if (!path || !content) return 0;
    FILE* f = fopen(path, "ab");
    if (!f) return 0;
    size_t len = strlen(content);
    fwrite(content, 1, len, f);
    fclose(f);
    return 1;
}

long long file_exists(long long path_val) {
    const char* path = (const char*)path_val;
    if (!path) return 0;
    FILE* f = fopen(path, "r");
    if (f) { fclose(f); return 1; }
    return 0;
}

long long string_contains(long long s_val, long long sub_val) {
    const char* s = (const char*)s_val;
    const char* sub = (const char*)sub_val;
    if (!s || !sub) return 0;
    return strstr(s, sub) != NULL ? 1 : 0;
}

long long string_index_of(long long s_val, long long sub_val) {
    const char* s = (const char*)s_val;
    const char* sub = (const char*)sub_val;
    if (!s || !sub) return -1;
    const char* found = strstr(s, sub);
    if (!found) return -1;
    return (long long)(found - s);
}

long long string_replace(long long s_val, long long old_val, long long new_val) {
    const char* s = (const char*)s_val;
    const char* old_str = (const char*)old_val;
    const char* new_str = (const char*)new_val;
    if (!s || !old_str || !new_str) return (long long)strdup(s ? s : "");
    size_t old_len = strlen(old_str);
    size_t new_len = strlen(new_str);
    if (old_len == 0) return (long long)strdup(s);
    int count = 0;
    const char* p = s;
    while ((p = strstr(p, old_str)) != NULL) { count++; p += old_len; }
    size_t result_len = strlen(s) + count * (new_len - old_len);
    char* result = malloc(result_len + 1);
    if (!result) return (long long)strdup(s);
    char* dst = result;
    p = s;
    while (*p) {
        if (strncmp(p, old_str, old_len) == 0) {
            memcpy(dst, new_str, new_len);
            dst += new_len;
            p += old_len;
        } else {
            *dst++ = *p++;
        }
    }
    *dst = '\0';
    ep_gc_register(result, EP_OBJ_STRING);
    return (long long)result;
}

/* ========== Additional String Functions ========== */
#include <ctype.h>

long long string_upper(long long s_val) {
    const char* s = (const char*)s_val;
    if (!s) return (long long)strdup("");
    long long len = strlen(s);
    char* result = malloc(len + 1);
    for (long long i = 0; i < len; i++) result[i] = toupper((unsigned char)s[i]);
    result[len] = '\0';
    ep_gc_register(result, EP_OBJ_STRING);
    return (long long)result;
}

long long string_lower(long long s_val) {
    const char* s = (const char*)s_val;
    if (!s) return (long long)strdup("");
    long long len = strlen(s);
    char* result = malloc(len + 1);
    for (long long i = 0; i < len; i++) result[i] = tolower((unsigned char)s[i]);
    result[len] = '\0';
    ep_gc_register(result, EP_OBJ_STRING);
    return (long long)result;
}

long long string_trim(long long s_val) {
    const char* s = (const char*)s_val;
    if (!s) return (long long)strdup("");
    while (*s && isspace((unsigned char)*s)) s++;
    long long len = strlen(s);
    while (len > 0 && isspace((unsigned char)s[len - 1])) len--;
    char* result = malloc(len + 1);
    memcpy(result, s, len);
    result[len] = '\0';
    ep_gc_register(result, EP_OBJ_STRING);
    return (long long)result;
}

long long string_split(long long s_val, long long delim_val) {
    const char* s = (const char*)s_val;
    const char* delim = (const char*)delim_val;
    if (!s || !delim) return create_list();
    long long list = create_list();
    long long dlen = strlen(delim);
    if (dlen == 0) { append_list(list, s_val); return list; }
    const char* p = s;
    while (1) {
        const char* found = strstr(p, delim);
        long long partlen = found ? (found - p) : (long long)strlen(p);
        char* part = malloc(partlen + 1);
        memcpy(part, p, partlen);
        part[partlen] = '\0';
        ep_gc_register(part, EP_OBJ_STRING);
        append_list(list, (long long)part);
        if (!found) break;
        p = found + dlen;
    }
    return list;
}

long long char_at(long long s_val, long long index) {
    const char* s = (const char*)s_val;
    if (!s || index < 0 || index >= (long long)strlen(s)) return 0;
    return (unsigned char)s[index];
}

long long char_from_code(long long code) {
    char* result = malloc(2);
    result[0] = (char)code;
    result[1] = '\0';
    ep_gc_register(result, EP_OBJ_STRING);
    return (long long)result;
}

long long ep_abs(long long n) {
    return n < 0 ? -n : n;
}

// Auto-convert any value to string for string interpolation
long long ep_auto_to_string(long long val) {
    // If the value is 0, return "0"
    if (val == 0) return (long long)strdup("0");
    // Check if val is a GC-tracked string (heap-allocated)
    EpGCObject* obj = ep_gc_find((void*)val);
    if (obj && obj->kind == EP_OBJ_STRING) {
        return val; // It's a known string pointer
    }
    // Check if val is a static string literal (in .rodata/.data segment)
    // These aren't GC-tracked but ARE valid pointers. Use a safe probe:
    // only dereference if the address is in a readable memory page.
    if (val > 0x100000) {
#if defined(_WIN32)
        // Windows: use VirtualQuery to safely probe pointer validity
        MEMORY_BASIC_INFORMATION mbi;
        if (VirtualQuery((void*)val, &mbi, sizeof(mbi)) && mbi.State == MEM_COMMIT && !(mbi.Protect & (PAGE_NOACCESS | PAGE_GUARD))) {
            const char* p = (const char*)(void*)val;
            unsigned char first = (unsigned char)*p;
            if ((first >= 0x20 && first <= 0x7E) || (first >= 0xC0 && first <= 0xFD) || first == '\n' || first == '\t' || first == '\r' || first == 0) {
                return val; // Readable memory, looks like a string
            }
        }
#elif defined(__APPLE__)
        // macOS: use vm_read_overwrite to safely probe
        char probe;
        vm_size_t sz = 1;
        kern_return_t kr = vm_read_overwrite(mach_task_self(), (mach_vm_address_t)val, 1, (mach_vm_address_t)&probe, &sz);
        if (kr == KERN_SUCCESS) {
            unsigned char first = (unsigned char)probe;
            if ((first >= 0x20 && first <= 0x7E) || (first >= 0xC0 && first <= 0xFD) || first == '\n' || first == '\t' || first == '\r' || first == 0) {
                return val; // Readable memory, looks like a string
            }
        }
#else
        // Linux: use write() to /dev/null as a safe pointer probe
        // write() returns -1 with EFAULT for invalid pointers, no signal
        int devnull = open("/dev/null", 1); // O_WRONLY
        if (devnull >= 0) {
            ssize_t r = write(devnull, (const void*)val, 1);
            close(devnull);
            if (r == 1) {
                const char* p = (const char*)(void*)val;
                unsigned char first = (unsigned char)*p;
                if ((first >= 0x20 && first <= 0x7E) || (first >= 0xC0 && first <= 0xFD) || first == '\n' || first == '\t' || first == '\r' || first == 0) {
                    return val;
                }
            }
        }
#endif
    }
    // Otherwise, convert integer to string
    char* buf = (char*)malloc(32);
    snprintf(buf, 32, "%lld", val);
    ep_gc_register(buf, EP_OBJ_STRING);
    return (long long)buf;
}

long long ep_random_int(long long min, long long max) {
    if (max <= min) return min;
    /* Draw from the OS CSPRNG with rejection sampling to avoid modulo bias. */
    unsigned long long range = (unsigned long long)(max - min) + 1ULL;
    unsigned long long limit = UINT64_MAX - (UINT64_MAX % range);
    unsigned long long r;
    do {
        ep_secure_random_bytes((unsigned char*)&r, sizeof(r));
    } while (r >= limit);
    return min + (long long)(r % range);
}

// JSON built-in functions
static const char* json_skip_ws(const char* p) {
    while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
    return p;
}

static const char* json_skip_value(const char* p) {
    p = json_skip_ws(p);
    if (*p == '"') {
        p++;
        while (*p && *p != '"') { if (*p == '\\') p++; p++; }
        if (*p == '"') p++;
    } else if (*p == '{') {
        int depth = 1; p++;
        while (*p && depth > 0) {
            if (*p == '"') { p++; while (*p && *p != '"') { if (*p == '\\') p++; p++; } if (*p) p++; }
            else if (*p == '{') { depth++; p++; }
            else if (*p == '}') { depth--; p++; }
            else p++;
        }
    } else if (*p == '[') {
        int depth = 1; p++;
        while (*p && depth > 0) {
            if (*p == '"') { p++; while (*p && *p != '"') { if (*p == '\\') p++; p++; } if (*p) p++; }
            else if (*p == '[') { depth++; p++; }
            else if (*p == ']') { depth--; p++; }
            else p++;
        }
    } else {
        while (*p && *p != ',' && *p != '}' && *p != ']' && *p != ' ' && *p != '\n') p++;
    }
    return p;
}

static const char* json_find_key(const char* json, const char* key) {
    const char* p = json_skip_ws(json);
    if (*p != '{') return NULL;
    p++;
    while (*p) {
        p = json_skip_ws(p);
        if (*p == '}') return NULL;
        if (*p != '"') return NULL;
        p++;
        const char* ks = p;
        while (*p && *p != '"') { if (*p == '\\') p++; p++; }
        size_t klen = p - ks;
        if (*p == '"') p++;
        p = json_skip_ws(p);
        if (*p == ':') p++;
        p = json_skip_ws(p);
        if (klen == strlen(key) && strncmp(ks, key, klen) == 0) {
            return p;
        }
        p = json_skip_value(p);
        p = json_skip_ws(p);
        if (*p == ',') p++;
    }
    return NULL;
}

long long json_get_string(long long json_val, long long key_val) {
    const char* json = (const char*)json_val;
    const char* key = (const char*)key_val;
    if (!json || !key) return (long long)strdup("");
    const char* val = json_find_key(json, key);
    if (!val || *val != '"') return (long long)strdup("");
    val++;
    const char* end = val;
    while (*end && *end != '"') { if (*end == '\\') end++; end++; }
    size_t len = end - val;
    char* result = (char*)malloc(len + 1);
    // Handle escape sequences
    size_t di = 0;
    const char* si = val;
    while (si < end) {
        if (*si == '\\' && si + 1 < end) {
            si++;
            switch (*si) {
                case 'n': result[di++] = '\n'; break;
                case 't': result[di++] = '\t'; break;
                case 'r': result[di++] = '\r'; break;
                case '"': result[di++] = '"'; break;
                case '\\': result[di++] = '\\'; break;
                default: result[di++] = *si; break;
            }
        } else {
            result[di++] = *si;
        }
        si++;
    }
    result[di] = '\0';
    ep_gc_register(result, EP_OBJ_STRING);
    return (long long)result;
}

long long json_get_int(long long json_val, long long key_val) {
    const char* json = (const char*)json_val;
    const char* key = (const char*)key_val;
    if (!json || !key) return 0;
    const char* val = json_find_key(json, key);
    if (!val) return 0;
    return atoll(val);
}

long long json_get_bool(long long json_val, long long key_val) {
    const char* json = (const char*)json_val;
    const char* key = (const char*)key_val;
    if (!json || !key) return 0;
    const char* val = json_find_key(json, key);
    if (!val) return 0;
    if (strncmp(val, "true", 4) == 0) return 1;
    return 0;
}

// SHA-1 implementation (RFC 3174) for WebSocket handshake
static unsigned int sha1_left_rotate(unsigned int x, int n) {
    return (x << n) | (x >> (32 - n));
}

long long ep_sha1(long long data_val) {
    const unsigned char* data = (const unsigned char*)data_val;
    if (!data) return (long long)strdup("");
    size_t len = strlen((const char*)data);

    unsigned int h0 = 0x67452301, h1 = 0xEFCDAB89, h2 = 0x98BADCFE, h3 = 0x10325476, h4 = 0xC3D2E1F0;
    size_t new_len = len + 1;
    while (new_len % 64 != 56) new_len++;
    unsigned char* msg = (unsigned char*)calloc(new_len + 8, 1);
    memcpy(msg, data, len);
    msg[len] = 0x80;
    unsigned long long bits_len = (unsigned long long)len * 8;
    for (int i = 0; i < 8; i++) msg[new_len + 7 - i] = (unsigned char)(bits_len >> (i * 8));

    for (size_t offset = 0; offset < new_len + 8; offset += 64) {
        unsigned int w[80];
        for (int i = 0; i < 16; i++) {
            w[i] = ((unsigned int)msg[offset + i*4] << 24) | ((unsigned int)msg[offset + i*4+1] << 16) |
                    ((unsigned int)msg[offset + i*4+2] << 8) | (unsigned int)msg[offset + i*4+3];
        }
        for (int i = 16; i < 80; i++) w[i] = sha1_left_rotate(w[i-3] ^ w[i-8] ^ w[i-14] ^ w[i-16], 1);
        unsigned int a = h0, b = h1, c = h2, d = h3, e = h4;
        for (int i = 0; i < 80; i++) {
            unsigned int f, k;
            if (i < 20) { f = (b & c) | ((~b) & d); k = 0x5A827999; }
            else if (i < 40) { f = b ^ c ^ d; k = 0x6ED9EBA1; }
            else if (i < 60) { f = (b & c) | (b & d) | (c & d); k = 0x8F1BBCDC; }
            else { f = b ^ c ^ d; k = 0xCA62C1D6; }
            unsigned int temp = sha1_left_rotate(a, 5) + f + e + k + w[i];
            e = d; d = c; c = sha1_left_rotate(b, 30); b = a; a = temp;
        }
        h0 += a; h1 += b; h2 += c; h3 += d; h4 += e;
    }
    free(msg);

    // Return Base64-encoded hash directly (for WebSocket handshake)
    unsigned char hash[20];
    hash[0] = (h0>>24)&0xFF; hash[1] = (h0>>16)&0xFF; hash[2] = (h0>>8)&0xFF; hash[3] = h0&0xFF;
    hash[4] = (h1>>24)&0xFF; hash[5] = (h1>>16)&0xFF; hash[6] = (h1>>8)&0xFF; hash[7] = h1&0xFF;
    hash[8] = (h2>>24)&0xFF; hash[9] = (h2>>16)&0xFF; hash[10] = (h2>>8)&0xFF; hash[11] = h2&0xFF;
    hash[12] = (h3>>24)&0xFF; hash[13] = (h3>>16)&0xFF; hash[14] = (h3>>8)&0xFF; hash[15] = h3&0xFF;
    hash[16] = (h4>>24)&0xFF; hash[17] = (h4>>16)&0xFF; hash[18] = (h4>>8)&0xFF; hash[19] = h4&0xFF;

    // Base64 encode the 20-byte hash
    size_t b64_len = 4 * ((20 + 2) / 3);
    char* result = (char*)malloc(b64_len + 1);
    size_t j = 0;
    for (size_t bi = 0; bi < 20; bi += 3) {
        unsigned int n2 = ((unsigned int)hash[bi]) << 16;
        if (bi + 1 < 20) n2 |= ((unsigned int)hash[bi+1]) << 8;
        if (bi + 2 < 20) n2 |= (unsigned int)hash[bi+2];
        result[j++] = b64_table[(n2 >> 18) & 0x3F];
        result[j++] = b64_table[(n2 >> 12) & 0x3F];
        result[j++] = (bi + 1 < 20) ? b64_table[(n2 >> 6) & 0x3F] : '=';
        result[j++] = (bi + 2 < 20) ? b64_table[n2 & 0x3F] : '=';
    }
    result[j] = '\0';
    ep_gc_register(result, EP_OBJ_STRING);
    return (long long)result;
}

// Read exact N bytes from a socket
#ifdef __wasm__
long long ep_net_recv_bytes(long long fd, long long count) {
    (void)fd; (void)count;
    return (long long)strdup("");
}
#else
long long ep_net_recv_bytes(long long fd, long long count) {
    if (count <= 0) return (long long)strdup("");
    char* buf = (char*)malloc(count + 1);
#ifdef _WIN32
    int total = 0;
    while (total < (int)count) {
        int n = recv((int)fd, buf + total, (int)(count - total), 0);
        if (n <= 0) break;
        total += n;
    }
#else
    ssize_t total = 0;
    while (total < count) {
        ssize_t n = recv((int)fd, buf + total, count - total, 0);
        if (n <= 0) break;
        total += n;
    }
#endif
    buf[total] = '\0';
    ep_gc_register(buf, EP_OBJ_STRING);
    return (long long)buf;
}
#endif

long long ep_get_args(void) {
    long long list_ptr = create_list();
    for (int i = 0; i < ep_argc; i++) {
        char* arg_copy = strdup(ep_argv[i]);
        ep_gc_register(arg_copy, EP_OBJ_STRING);
        append_list(list_ptr, (long long)arg_copy);
    }
    return list_ptr;
}

