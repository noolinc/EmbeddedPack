/* EMB-DEVIATION(EMB-PRC-002): host-side demonstration fixture, not product code, so it traces to no requirement */
#include <stdbool.h>
#include <stdint.h>
/* EMB-DEVIATION(EMB-C-007): host test that reports its verdict to CTest on stdout and stderr */
#include <stdio.h>
#include <stdlib.h>

#define WRAP_SAFE_COUNTER 1

enum { HEARTBEAT_TIMEOUT = 5 };

/* Timestamps and elapsed time use the same 32-bit clock. The caller must
 * service the heartbeat within one full counter period.
 */
static bool heartbeat_expired(uint32_t now, uint32_t last_heartbeat)
{
    if (!WRAP_SAFE_COUNTER && now < last_heartbeat) {
        return true; /* A synthetic regression treats rollover as a fault. */
    }

    const uint32_t elapsed = (uint32_t)(now - last_heartbeat);
    return elapsed >= HEARTBEAT_TIMEOUT;
}

int main(void)
{
    const uint32_t last_heartbeat = UINT32_MAX - 2u;

    for (uint32_t elapsed = 0; elapsed < HEARTBEAT_TIMEOUT; ++elapsed) {
        if (heartbeat_expired((uint32_t)(last_heartbeat + elapsed), last_heartbeat)) {
            fprintf(stderr, "premature failsafe across rollover at tick %lu\n",
                    (unsigned long)elapsed);
            return EXIT_FAILURE;
        }
    }

    if (!heartbeat_expired((uint32_t)(last_heartbeat + HEARTBEAT_TIMEOUT),
                           last_heartbeat)) {
        fputs("missed the real heartbeat timeout after rollover\n", stderr);
        return EXIT_FAILURE;
    }
    puts("rollover did not trigger failsafe; the real timeout still fired");
    return EXIT_SUCCESS;
}
