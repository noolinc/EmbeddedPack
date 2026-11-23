/* EMB-DEVIATION(EMB-PRC-002): host-side demonstration fixture, not product code, so it traces to no requirement */
#include <stdbool.h>
#include <stdint.h>
/* EMB-DEVIATION(EMB-C-007): host test that reports its verdict to CTest on stdout and stderr */
#include <stdio.h>
#include <stdlib.h>

#define REFILL_ON_TX_EMPTY 1

enum {
    BYTE_TICKS = 10,
    BYTE_COUNT = 4,
    MAX_IDLE_TICKS = 0,
    SIMULATION_TICKS = (BYTE_TICKS + 2) * BYTE_COUNT + 10,
};

static const uint8_t message[BYTE_COUNT] = {0x55, 0xa3, 0x00, 0x7f};

struct uart {
    uint8_t data;
    bool data_pending;
    bool irq_enabled;
    unsigned shift_remaining;
    unsigned next_byte;
    unsigned completion;
    unsigned wire_count;
    uint8_t wire[BYTE_COUNT];
    unsigned start[BYTE_COUNT];
};

static void queue_byte(struct uart *uart)
{
    uart->data = message[uart->next_byte++];
    uart->data_pending = true;
}

static bool tx_complete(const struct uart *uart)
{
    return uart->shift_remaining == 0 && !uart->data_pending;
}

static void transmit_tick(struct uart *uart, unsigned tick)
{
    /* Hardware moves queued data into its shifter before the CPU services
     * the UART. Writing the data register from the ISR takes effect on the
     * next tick.
     */
    if (uart->shift_remaining == 0 && uart->data_pending) {
        uart->wire[uart->wire_count] = uart->data;
        uart->start[uart->wire_count++] = tick;
        uart->data_pending = false;
        uart->shift_remaining = BYTE_TICKS;
    }

    const bool refill_ready = !uart->data_pending &&
                              (REFILL_ON_TX_EMPTY || tx_complete(uart));
    if (uart->irq_enabled && uart->next_byte < BYTE_COUNT && refill_ready) {
        queue_byte(uart);
    } else if (uart->irq_enabled && uart->next_byte == BYTE_COUNT &&
               tx_complete(uart)) {
        uart->completion = tick;
        uart->irq_enabled = false;
    }

    if (uart->shift_remaining > 0) {
        --uart->shift_remaining;
    }
}

int main(void)
{
    struct uart uart = {.irq_enabled = true};
    queue_byte(&uart); /* The timer interrupt queues the first byte. */

    for (unsigned tick = 0; tick < SIMULATION_TICKS; ++tick) {
        transmit_tick(&uart, tick);
    }

    if (uart.irq_enabled || uart.wire_count != BYTE_COUNT ||
        uart.completion != uart.start[BYTE_COUNT - 1] + BYTE_TICKS) {
        fputs("TX must finish its last byte and disable interrupts\n", stderr);
        return EXIT_FAILURE;
    }

    for (unsigned i = 0; i < BYTE_COUNT; ++i) {
        if (uart.wire[i] != message[i]) {
            fprintf(stderr, "wrong byte on the wire at index %u\n", i);
            return EXIT_FAILURE;
        }
        if (i > 0 && uart.start[i] > uart.start[i - 1] + BYTE_TICKS + MAX_IDLE_TICKS) {
            fprintf(stderr, "inter-byte gap exceeds %u idle ticks at index %u\n",
                    (unsigned)MAX_IDLE_TICKS, i);
            return EXIT_FAILURE;
        }
    }

    puts("bytes arrived in order without idle gaps; TX stopped after the last byte");
    return EXIT_SUCCESS;
}
