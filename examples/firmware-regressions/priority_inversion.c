/* EMB-DEVIATION(EMB-PRC-002): host-side demonstration fixture, not product code, so it traces to no requirement */
/* EMB-DEVIATION(EMB-C-007): host test that reports its verdict to CTest on stdout and stderr */
#include <stdio.h>
#include <stdlib.h>

#define PRIORITY_INHERITANCE 1

enum {
    LOW = 1,
    MEDIUM = 2,
    HIGH = 3,
    HOLDER_WORK = 2,
    CPU_LOAD = 6,
    LOCK_DEADLINE = 3,
};

/* The low-priority task starts with the lock. The high-priority task blocks
 * on it, and an unrelated task of medium priority becomes runnable.
 */
static unsigned lock_wait_ticks(void)
{
    const unsigned holder_priority = PRIORITY_INHERITANCE ? HIGH : LOW;
    unsigned holder_remaining = HOLDER_WORK;
    unsigned medium_remaining = CPU_LOAD;
    unsigned ticks = 0;

    while (holder_remaining > 0) {
        if (medium_remaining > 0 && MEDIUM > holder_priority) {
            --medium_remaining;
        } else {
            --holder_remaining;
        }
        ++ticks;
    }
    return ticks;
}

int main(void)
{
    unsigned wait = lock_wait_ticks();

    if (wait > LOCK_DEADLINE) {
        fprintf(stderr, "lock deadline missed: waited %u ticks (limit %u)\n",
                wait, (unsigned)LOCK_DEADLINE);
        return EXIT_FAILURE;
    }
    printf("high-priority waiter acquired the lock in %u ticks\n", wait);
    return EXIT_SUCCESS;
}
