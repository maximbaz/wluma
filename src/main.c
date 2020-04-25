#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wayland-client.h>

#include "wlr-export-dmabuf-unstable-v1-client-protocol.h"


struct context {
    struct wl_display *display;
    struct wl_list outputs;
    struct zwlr_export_dmabuf_manager_v1 *dmabuf_manager;
};

struct wayland_output {
    struct wl_output *output;
    struct wl_list link;
    uint32_t id;
    int width;
    int height;
};

static void nop() {}


/******************************************************************************
 * Outputs management
 */
static void output_handle_mode(void *data, struct wl_output *wl_output, uint32_t flags, int32_t width, int32_t height, int32_t refresh) {
    if (flags & WL_OUTPUT_MODE_CURRENT) {
        struct wayland_output *output = data;
        output->width = width;
        output->height = height;
    }
}

static const struct wl_output_listener output_listener = {
    .mode = output_handle_mode,
    .geometry = nop,
    .done = nop,
    .scale = nop,
};

static void remove_output(struct wayland_output *out) {
    wl_list_remove(&out->link);
}

static struct wayland_output* find_output(struct context *ctx, struct wl_output *out, uint32_t id) {
    struct wayland_output *output, *tmp;
    wl_list_for_each_safe(output, tmp, &ctx->outputs, link) {
        if ((output->output == out) || (output->id == id)) {
            return output;
        }
    }
    return NULL;
}

static void registry_handle_remove(void *data, struct wl_registry *reg, uint32_t id) {
    remove_output(find_output((struct context*)data, NULL, id));
}

static void registry_handle_add(void *data, struct wl_registry *reg, uint32_t id, const char *interface, uint32_t ver) {
    struct context *ctx = data;

    if (strcmp(interface, wl_output_interface.name) == 0) {
        struct wayland_output *output = malloc(sizeof(struct wayland_output));

        output->id = id;
        output->output = wl_registry_bind(reg, id, &wl_output_interface, ver);

        wl_output_add_listener(output->output, &output_listener, output);
        wl_list_insert(&ctx->outputs, &output->link);
    }

    if (strcmp(interface, zwlr_export_dmabuf_manager_v1_interface.name) == 0) {
        ctx->dmabuf_manager = wl_registry_bind(reg, id, &zwlr_export_dmabuf_manager_v1_interface, ver);
    }
}


/******************************************************************************
 * Initialize display, register an outputs manager
 */
static int init(struct context *ctx) {
    ctx->display = wl_display_connect(NULL);
    if (!ctx->display) {
        printf("ERROR: Failed to connect to display!\n");
        return 1;
    }

    wl_list_init(&ctx->outputs);

    struct wl_registry *registry = wl_display_get_registry(ctx->display);

    struct wl_registry_listener listener = {
        .global = registry_handle_add,
        .global_remove = registry_handle_remove,
    };
    wl_registry_add_listener(registry, &listener, ctx);

    wl_display_roundtrip(ctx->display);
    wl_display_dispatch(ctx->display);

    return 0;
}


/******************************************************************************
 * Main
 */

int main() {
    struct context ctx = { 0 };

    int err = init(&ctx);
    if (err) {
        return err;
    }

    return 0;
}
