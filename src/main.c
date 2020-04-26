#define _POSIX_C_SOURCE 200809L

#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <time.h>

#include <wayland-client.h>
#include <wlr/render/gles2.h>
#include <GLES2/gl2ext.h>

#include "wlr-export-dmabuf-unstable-v1-client-protocol.h"

struct frame {
    struct zwlr_export_dmabuf_frame_v1* frame;
    uint32_t format;
    uint32_t width;
    uint32_t height;
    uint32_t num_objects;
    uint32_t flags;
    uint64_t format_modifier;

    uint32_t strides[4];
    uint32_t sizes[4];
    int32_t  fds[4];
    uint32_t offsets[4];
    uint32_t plane_indices[4];
};

struct context {
    struct wl_display *display;
    struct wl_list outputs;
    struct zwlr_export_dmabuf_manager_v1 *dmabuf_manager;

    // Target
    struct wayland_output *target_output;
    bool with_cursor;

    // Main frame callback
    struct zwlr_export_dmabuf_frame_v1 *frame_callback;

    // Frames
    struct frame *current_frame, *next_frame;

    // EGL
    struct wlr_egl egl;
    PFNGLEGLIMAGETARGETTEXTURE2DOESPROC glEGLImageTargetTexture2DOES;

    // Errors
    bool quit;
    int err;
};

struct wayland_output {
    struct wl_output *output;
    struct wl_list link;
    uint32_t id;
};


/******************************************************************************
 * Frame management
 */
static void register_frame_listener(struct context *ctx);

static void frame_free(struct frame *frame) {
    if (frame == NULL) {
        return;
    }

    zwlr_export_dmabuf_frame_v1_destroy(frame->frame);
    for (uint32_t i = 0; i < frame->num_objects; i++) {
        close(frame->fds[i]);
    }
    free(frame);
}

static void frame_start(void *data, struct zwlr_export_dmabuf_frame_v1 *frame,
        uint32_t width, uint32_t height, uint32_t offset_x, uint32_t offset_y,
        uint32_t buffer_flags, uint32_t flags, uint32_t format,
        uint32_t mod_high, uint32_t mod_low, uint32_t num_objects) {
    struct context *ctx = data;
    ctx->next_frame = calloc(1, sizeof(struct frame));
    ctx->next_frame->frame = frame;
    ctx->next_frame->width = width;
    ctx->next_frame->height = height;
    ctx->next_frame->format = format;
    ctx->next_frame->format_modifier = ((uint64_t)mod_high << 32) | mod_low;
    ctx->next_frame->num_objects = num_objects;
    ctx->next_frame->flags = flags;
}

static void frame_object(void *data, struct zwlr_export_dmabuf_frame_v1 *frame,
        uint32_t index, int32_t fd, uint32_t size, uint32_t offset,
        uint32_t stride, uint32_t plane_index) {
    struct context *ctx = data;
    ctx->next_frame->fds[index] = fd;
    ctx->next_frame->sizes[index] = size;
    ctx->next_frame->strides[index] = stride;
    ctx->next_frame->offsets[index] = offset;
    ctx->next_frame->plane_indices[index] = plane_index;
}

static void frame_ready(void *data, struct zwlr_export_dmabuf_frame_v1 *frame,
        uint32_t tv_sec_hi, uint32_t tv_sec_lo, uint32_t tv_nsec) {
    struct context *ctx = data;

    // Re-create expected attributes structure
    struct wlr_dmabuf_attributes attribs = {
        .width = ctx->next_frame->width,
        .height = ctx->next_frame->height,
        .format = ctx->next_frame->format,
        .flags = ctx->next_frame->flags,
        .modifier = ctx->next_frame->format_modifier,
        .n_planes = ctx->next_frame->num_objects,
    };
    memcpy(attribs.offset, ctx->next_frame->offsets, sizeof(attribs.offset));
    memcpy(attribs.stride, ctx->next_frame->strides, sizeof(attribs.stride));
    memcpy(attribs.fd, ctx->next_frame->fds, sizeof(attribs.fd));

    // Create an image of the current frame
    EGLImageKHR img = wlr_egl_create_image_from_dmabuf(&ctx->egl, &attribs);

    // Create a texture to hold the frame
    GLuint texture;
    glGenTextures(1, &texture);
    glBindTexture(GL_TEXTURE_2D, texture);

    // Convert the image to a texture we can later use
    ctx->glEGLImageTargetTexture2DOES(GL_TEXTURE_2D, img);

    // Generate mipmaps
    glGenerateMipmap(GL_TEXTURE_2D);

    // Compute the level of the smallest 1x1 mipmap level, containing average pixel value for the entire frame
    double smallestMipmapLevel = floor(log2(fmax(ctx->next_frame->width, ctx->next_frame->height)));

    // glGetTexImage() seems to be unavailable, so we can't read out the mipmap values directly :(

    // Prepare a framebuffer to draw out mipmap on
    GLuint framebuffer;
    glGenFramebuffers(1, &framebuffer);
    glBindFramebuffer(GL_FRAMEBUFFER, framebuffer);

    // Draw smallest mipmap level of our texture onto the framebuffer
    glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, texture, smallestMipmapLevel);

    // Read out the one and only pixel from the framebuffer
    GLubyte pixels[] = {0, 0, 0, 0};
    glReadPixels(0, 0, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, pixels);

    // DEBUG check
    printf("RGB=#%x%x%x A=%d\n", pixels[0], pixels[1], pixels[2], pixels[3]);

    // Cleanup
    glBindFramebuffer(GL_FRAMEBUFFER, 0);
    glBindTexture(GL_TEXTURE_2D, 0);
    glDeleteTextures(1, &texture);
    frame_free(ctx->current_frame);
    ctx->current_frame = ctx->next_frame;
    ctx->next_frame = NULL;

    if (!ctx->quit && !ctx->err) {
        // Sleep a bit before asking for the next frame
        struct timespec ts;
        ts.tv_sec = 0;
        ts.tv_nsec = 100 * 1000 * 1000;
        nanosleep(&ts, NULL);

        // Ask for the next frame
        register_frame_listener(ctx);
    }
}

static void frame_cancel(void *data, struct zwlr_export_dmabuf_frame_v1 *frame,
        uint32_t reason) {
    struct context *ctx = data;
    frame_free(ctx->next_frame);
    if (reason == ZWLR_EXPORT_DMABUF_FRAME_V1_CANCEL_REASON_PERMANENT) {
        printf("ERROR: Permanent failure when capturing frame!\n");
        ctx->err = true;
    } else {
        register_frame_listener(ctx);
    }
}

static const struct zwlr_export_dmabuf_frame_v1_listener frame_listener = {
    .frame = frame_start,
    .object = frame_object,
    .ready = frame_ready,
    .cancel = frame_cancel,
};

static void register_frame_listener(struct context *ctx) {
    ctx->frame_callback = zwlr_export_dmabuf_manager_v1_capture_output(ctx->dmabuf_manager, ctx->with_cursor, ctx->target_output->output);
    zwlr_export_dmabuf_frame_v1_add_listener(ctx->frame_callback, &frame_listener, ctx);
}


/******************************************************************************
 * Outputs management
 */
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

        wl_list_insert(&ctx->outputs, &output->link);
    }

    if (strcmp(interface, zwlr_export_dmabuf_manager_v1_interface.name) == 0) {
        ctx->dmabuf_manager = wl_registry_bind(reg, id, &zwlr_export_dmabuf_manager_v1_interface, ver);
    }
}


/******************************************************************************
 * Main loop
 */
struct context *quit_ctx = NULL;

static void on_quit_signal(int signal) {
    printf("\r");
    printf("Exiting on signal: %d\n", signal);
    quit_ctx->quit = true;
}

static int main_loop(struct context *ctx) {
    int err;

    quit_ctx = ctx;

    if (signal(SIGINT, on_quit_signal) == SIG_ERR) {
        printf("ERROR: Failed to install signal handler!\n");
        return 1;
    }

    register_frame_listener(ctx);

    // Run capture
    while (wl_display_dispatch(ctx->display) != -1 && !ctx->err && !ctx->quit);

    return ctx->err;
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

    if (wl_list_empty(&ctx->outputs)) {
        printf("ERROR: Failed to retrieve any output!\n");
        return 1;
    }

    if (!ctx->dmabuf_manager) {
        printf("ERROR: Failed to initialize DMA-BUF manager!\n");
        return 1;
    }

    if (!wlr_egl_init(&ctx->egl, EGL_PLATFORM_WAYLAND_EXT, ctx->display, NULL, WL_SHM_FORMAT_ARGB8888)) {
        printf("ERROR: Failed to initialize EGL!\n");
        return 1;
    }

    if (!wlr_gles2_renderer_create(&ctx->egl)) {
        printf("ERROR: Failed to initialize GLES2 renderer!\n");
        return 1;
    }

    *(void **)&ctx->glEGLImageTargetTexture2DOES = eglGetProcAddress("glEGLImageTargetTexture2DOES");
    if (ctx->glEGLImageTargetTexture2DOES == NULL) {
        printf("ERROR: Failed to load EGL proc glEGLImageTargetTexture2DOES!\n");
        return 1;
    }

    return 0;
}

static void deinit(struct context *ctx) {
    struct wayland_output *output, *tmp_o;
    wl_list_for_each_safe(output, tmp_o, &ctx->outputs, link) {
        remove_output(output);
    }

    if (ctx->dmabuf_manager) {
        zwlr_export_dmabuf_manager_v1_destroy(ctx->dmabuf_manager);
    }
}


/******************************************************************************
 * Main
 */

int main() {
    int err = 0;
    struct context ctx = { 0 };

    err = init(&ctx);
    if (err) {
        goto exit;
    }

    // TODO: handle multiple outputs
    struct wayland_output *o, *tmp_o;
    wl_list_for_each_safe(o, tmp_o, &ctx.outputs, link) {
        ctx.target_output = o;
    }

    err = main_loop(&ctx);
    if (err) {
        goto exit;
    }

exit:
    deinit(&ctx);
    return err;
}
