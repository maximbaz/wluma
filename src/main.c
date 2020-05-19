#define _POSIX_C_SOURCE 200809L

#include <dirent.h>
#include <fcntl.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include <time.h>
#include <vulkan/vulkan.h>
#include <wayland-client.h>

#include "wlr-export-dmabuf-unstable-v1-client-protocol.h"

#define FRAME_REQUEST_DELAY_NS        (100 * 1000000L)
#define BACKLIGHT_TRANSITION_DELAY_NS (2   * 1000000L)
#define VULKAN_FENCE_MAX_WAIT_NS      (100 * 1000000L)

struct DataPoint {
    struct DataPoint *next;
    struct DataPoint *prev;
    long lux;
    int luma;
    int backlight;
};

struct Vulkan {
    VkInstance instance;
    VkDevice device;
    VkQueue queue;
    VkCommandPool command_pool;
    VkCommandBuffer command_buffer;
    VkBuffer buffer;
    VkDeviceMemory buffer_memory;
    VkFence fence;
};

struct Frame {
    struct zwlr_export_dmabuf_frame_v1* frame;
    uint32_t width;
    uint32_t height;
    uint32_t num_objects;

    uint32_t sizes[4];
    int32_t  fds[4];
};

struct VulkanFrame {
    uint32_t mip_levels;
    VkImage image;
    VkDeviceMemory image_memory;
};

struct WaylandOutput {
    struct wl_output *output;
    struct wl_list link;
    uint32_t id;
};

struct Context {
    struct wl_display *display;
    struct wl_list *outputs;
    struct zwlr_export_dmabuf_manager_v1 *dmabuf_manager;

    // Target
    struct WaylandOutput *target_output;

    // Main frame callback
    struct zwlr_export_dmabuf_frame_v1 *frame_callback;

    // Vulkan context
    struct Vulkan *vulkan;

    // DMA-BUF frame
    struct Frame *frame;

    // Vulkan structs for processing frames, might be reused
    struct VulkanFrame *vulkan_frame;

    // Ambient light sensor raw data
    int light_sensor_raw_fd;
    double light_sensor_scale;
    double light_sensor_offset;
    long lux_max_seen;

    // Backlight control
    int backlight_raw_fd;
    long backlight_max;
    int backlight_last;

    // Data points to determine the best backlight value
    int data_fd;
    struct DataPoint *data;

    // Errors
    bool quit;
    int err;
};


/******************************************************************************
 * Utilities
 */

static double pread_double(int fd) {
    char buf[50];
    if (pread(fd, buf, 50, 0) < 0) {
        return -1;
    }
    return strtod(buf, NULL);
}

static void pwrite_long(int fd, long val) {
    char buf[50];
    int len = sprintf(buf, "%ld", val);
    ftruncate(fd, 0);
    pwrite(fd, buf, len, 0);
}

static char* get_env(char *name, char *def) {
    char *val = getenv(name);
    return val ? val : def;
}

/******************************************************************************
 * Data points
 */

static struct DataPoint* data_add(struct Context *ctx, long lux, int luma, int backlight) {
    struct DataPoint *point = malloc(sizeof(struct DataPoint));
    point->lux = lux;
    point->luma = luma;
    point->backlight = backlight;

    if (ctx->data) {
        struct DataPoint *next = ctx->data->next;
        ctx->data->next = point;
        point->next = next;
        point->prev = ctx->data;
        if (next) {
            next->prev = point;
        }
    } else {
        ctx->data = point;
        point->next = NULL;
        point->prev = NULL;
    }

    return point;
}

static struct DataPoint* data_remove(struct Context *ctx, struct DataPoint *point) {
    if (ctx->data == point) {
        ctx->data = ctx->data->next;
    }

    struct DataPoint *next = point->next;
    struct DataPoint *prev = point->prev;
    if (next) {
        next->prev = prev;
    }
    if (prev) {
        prev->next = next;
    }

    free(point);
    return next;
}

static void data_save(struct Context *ctx) {
    ftruncate(ctx->data_fd, 0);
    lseek(ctx->data_fd, 0, SEEK_SET);

    char buf[150];
    struct DataPoint *elem = ctx->data;
    while (elem) {
        int len = sprintf(buf, "%ld %d %d\n", elem->lux, elem->luma, elem->backlight);
        write(ctx->data_fd, buf, len);
        elem = elem->next;
    }
}

static bool data_load(struct Context *ctx) {
    FILE *f = fdopen(dup(ctx->data_fd), "r");
    if (f == NULL) {
        return false;
    }

    char line[150];
    while (fgets(line, 150, f)) {
        long val[3];
        char *word = NULL;
        for (int i=0; i<3; i++) {
            word = strtok(word == NULL ? line : NULL, " ");
            if (word == NULL) {
                return false;
            }
            val[i] = strtol(word, NULL, 10);
        }
        data_add(ctx, val[0], val[1], val[2]);
        ctx->lux_max_seen = fmax(fmax(ctx->lux_max_seen, val[0]), 1);
    }

    fclose(f);
    return true;
}


/******************************************************************************
 * Devices
 */

static long read_lux(struct Context *ctx) {
    return round((pread_double(ctx->light_sensor_raw_fd) + ctx->light_sensor_offset) * ctx->light_sensor_scale);
}

static int read_backlight_pct(struct Context *ctx) {
    return round(pread_double(ctx->backlight_raw_fd) * 100 / ctx->backlight_max);
}


/******************************************************************************
 * Vulkan
 */

static void init_frame_vulkan(struct Context *ctx) {
    if (ctx->vulkan_frame) {
        // TODO support resized frames
        return;
    }

    ctx->vulkan_frame = malloc(sizeof(struct VulkanFrame));

    ctx->vulkan_frame->mip_levels = floor(log2(fmax(ctx->frame->width, ctx->frame->height)));

    VkImageCreateInfo imageInfo = {
        .sType         = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
        .imageType     = VK_IMAGE_TYPE_2D,
        .format        = VK_FORMAT_B8G8R8A8_UNORM,
        .extent.width  = ctx->frame->width / 2,
        .extent.height = ctx->frame->height / 2,
        .extent.depth  = 1,
        .mipLevels     = ctx->vulkan_frame->mip_levels,
        .arrayLayers   = 1,
        .tiling        = VK_IMAGE_TILING_OPTIMAL,
        .initialLayout = VK_IMAGE_LAYOUT_UNDEFINED,
        .usage         = VK_IMAGE_USAGE_TRANSFER_DST_BIT | VK_IMAGE_USAGE_TRANSFER_SRC_BIT | VK_IMAGE_USAGE_SAMPLED_BIT,
        .sharingMode   = VK_SHARING_MODE_EXCLUSIVE,
        .samples       = VK_SAMPLE_COUNT_1_BIT,
    };

    if (vkCreateImage(ctx->vulkan->device, &imageInfo, NULL, &ctx->vulkan_frame->image) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to create Vulkan image!\n");
        goto fail;
    }

    VkMemoryRequirements imageMemoryRequirements;
    vkGetImageMemoryRequirements(ctx->vulkan->device, ctx->vulkan_frame->image, &imageMemoryRequirements);

    VkMemoryAllocateInfo imageMemoryAllocateInfo = {
        .sType           = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize  = imageMemoryRequirements.size,
        .memoryTypeIndex = 0,
    };

    if (vkAllocateMemory(ctx->vulkan->device, &imageMemoryAllocateInfo, NULL, &ctx->vulkan_frame->image_memory) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to allocate memory for Vulkan image!\n");
        goto fail;
    }

    if (vkBindImageMemory(ctx->vulkan->device, ctx->vulkan_frame->image, ctx->vulkan_frame->image_memory, 0) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to bind allocated memory for Vulkan image!\n");
        goto fail;
    }

    return;

fail:
    free(ctx->vulkan_frame);
    ctx->vulkan_frame = NULL;
}

static int compute_frame_luma_pct(struct Context *ctx) {
    int result = -1;

    if (ctx->vulkan_frame == NULL) {
        fprintf(stderr, "ERROR: Vulkan objects were not prepared, skipping frame!\n");
        goto exit;
    }

    VkExternalMemoryImageCreateInfo frameImageMemoryInfo = {
        .sType       = VK_STRUCTURE_TYPE_EXTERNAL_MEMORY_IMAGE_CREATE_INFO,
        .handleTypes = VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT,
    };

    VkImageCreateInfo frameImageInfo = {
        .sType         = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
        .pNext         = &frameImageMemoryInfo,
        .imageType     = VK_IMAGE_TYPE_2D,
        .format        = VK_FORMAT_R8G8B8A8_UNORM,
        .extent.width  = ctx->frame->width,
        .extent.height = ctx->frame->height,
        .extent.depth  = 1,
        .mipLevels     = 1,
        .arrayLayers   = 1,
        .flags         = VK_IMAGE_CREATE_ALIAS_BIT,
        .tiling        = VK_IMAGE_TILING_OPTIMAL,
        .initialLayout = VK_IMAGE_LAYOUT_UNDEFINED, // specs say so
        .usage         = VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_TRANSFER_SRC_BIT,
        .sharingMode   = VK_SHARING_MODE_EXCLUSIVE,
        .samples       = VK_SAMPLE_COUNT_1_BIT,
    };

    VkImage frameImage;
    if (vkCreateImage(ctx->vulkan->device, &frameImageInfo, NULL, &frameImage) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to create Vulkan frame image!\n");
        goto exit;
    }

    VkImportMemoryFdInfoKHR idesc = {
        .sType      = VK_STRUCTURE_TYPE_IMPORT_MEMORY_FD_INFO_KHR,
        .handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT,
        .fd         = dup(ctx->frame->fds[0]),
    };
    VkMemoryAllocateInfo alloc_info = {
        .sType           = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .pNext           = &idesc,
        .allocationSize = ctx->frame->sizes[0],
        .memoryTypeIndex = 0,
    };

    VkDeviceMemory frameImageMemory;
    if (vkAllocateMemory(ctx->vulkan->device, &alloc_info, NULL, &frameImageMemory) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to allocate memory for Vulkan frame image!\n");
        goto exit;
    }

    if (vkBindImageMemory(ctx->vulkan->device, frameImage, frameImageMemory, 0) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to bind allocated memory for Vulkan frame image!\n");
        goto exit;
    }

    VkCommandBufferBeginInfo commandBufferBeginInfo = {
        .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
        .flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT,
    };

    if (vkBeginCommandBuffer(ctx->vulkan->command_buffer, &commandBufferBeginInfo) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to begin Vulkan command buffer!\n");
        goto exit;
    }

    VkImageMemoryBarrier frameImageBarrier = {
        .sType                           = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
        .oldLayout                       = VK_IMAGE_LAYOUT_UNDEFINED,
        .newLayout                       = VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
        .srcQueueFamilyIndex             = VK_QUEUE_FAMILY_IGNORED,
        .dstQueueFamilyIndex             = VK_QUEUE_FAMILY_IGNORED,
        .image                           = frameImage,
        .subresourceRange.aspectMask     = VK_IMAGE_ASPECT_COLOR_BIT,
        .subresourceRange.baseArrayLayer = 0,
        .subresourceRange.baseMipLevel   = 0,
        .subresourceRange.layerCount     = 1,
        .subresourceRange.levelCount     = 1,
        .srcAccessMask                   = 0,
        .dstAccessMask                   = VK_ACCESS_TRANSFER_READ_BIT,
    };

    vkCmdPipelineBarrier(ctx->vulkan->command_buffer,
        VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT, 0,
        0, NULL,
        0, NULL,
        1, &frameImageBarrier);

    VkImageMemoryBarrier imageBarrier = {
        .sType                           = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
        .oldLayout                       = VK_IMAGE_LAYOUT_UNDEFINED,
        .newLayout                       = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
        .srcQueueFamilyIndex             = VK_QUEUE_FAMILY_IGNORED,
        .dstQueueFamilyIndex             = VK_QUEUE_FAMILY_IGNORED,
        .image                           = ctx->vulkan_frame->image,
        .subresourceRange.aspectMask     = VK_IMAGE_ASPECT_COLOR_BIT,
        .subresourceRange.baseArrayLayer = 0,
        .subresourceRange.baseMipLevel   = 0,
        .subresourceRange.layerCount     = 1,
        .subresourceRange.levelCount     = ctx->vulkan_frame->mip_levels,
        .srcAccessMask                   = 0,
        .dstAccessMask                   = VK_ACCESS_TRANSFER_WRITE_BIT,
    };

    vkCmdPipelineBarrier(ctx->vulkan->command_buffer,
        VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT, 0,
        0, NULL,
        0, NULL,
        1, &imageBarrier);

    VkImageBlit blit = {
        .srcOffsets[0]                 = { 0, 0, 0 },
        .srcOffsets[1]                 = { ctx->frame->width, ctx->frame->height, 1 },
        .srcSubresource.aspectMask     = VK_IMAGE_ASPECT_COLOR_BIT,
        .srcSubresource.mipLevel       = 0,
        .srcSubresource.baseArrayLayer = 0,
        .srcSubresource.layerCount     = 1,
        .dstOffsets[0]                 = { 0, 0, 0 },
        .dstOffsets[1]                 = { ctx->frame->width / 2, ctx->frame->height / 2, 1 },
        .dstSubresource.aspectMask     = VK_IMAGE_ASPECT_COLOR_BIT,
        .dstSubresource.mipLevel       = 0,
        .dstSubresource.baseArrayLayer = 0,
        .dstSubresource.layerCount     = 1,
    };

    vkCmdBlitImage(ctx->vulkan->command_buffer,
        frameImage, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
        ctx->vulkan_frame->image, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
        1, &blit,
        VK_FILTER_LINEAR);

    imageBarrier.subresourceRange.levelCount = 1;
    uint32_t mipWidth  = ctx->frame->width / 2;
    uint32_t mipHeight = ctx->frame->height / 2;

    for (uint32_t i = 1; i < ctx->vulkan_frame->mip_levels; i++) {
        imageBarrier.subresourceRange.baseMipLevel = i - 1;
        imageBarrier.oldLayout                     = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL;
        imageBarrier.newLayout                     = VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL;
        imageBarrier.srcAccessMask                 = VK_ACCESS_TRANSFER_WRITE_BIT;
        imageBarrier.dstAccessMask                 = VK_ACCESS_TRANSFER_READ_BIT;

        vkCmdPipelineBarrier(ctx->vulkan->command_buffer,
            VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT, 0,
            0, NULL,
            0, NULL,
            1, &imageBarrier);

        blit.srcOffsets[1] = (VkOffset3D) { mipWidth, mipHeight, 1 };
        blit.dstOffsets[1] = (VkOffset3D) { mipWidth > 1 ? mipWidth / 2 : 1, mipHeight > 1 ? mipHeight / 2 : 1, 1 };
        blit.srcSubresource.mipLevel = i - 1;
        blit.dstSubresource.mipLevel = i;

        vkCmdBlitImage(ctx->vulkan->command_buffer,
            ctx->vulkan_frame->image, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
            ctx->vulkan_frame->image, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
            1, &blit,
            VK_FILTER_LINEAR);

        if (mipWidth > 1)  mipWidth /= 2;
        if (mipHeight > 1) mipHeight /= 2;
    }

    imageBarrier.subresourceRange.baseMipLevel = ctx->vulkan_frame->mip_levels - 1;
    imageBarrier.oldLayout                     = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL;
    imageBarrier.newLayout                     = VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL;
    imageBarrier.srcAccessMask                 = VK_ACCESS_TRANSFER_WRITE_BIT;
    imageBarrier.dstAccessMask                 = VK_ACCESS_TRANSFER_READ_BIT;

    vkCmdPipelineBarrier(ctx->vulkan->command_buffer,
        VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT, 0,
        0, NULL,
        0, NULL,
        1, &imageBarrier);

    VkBufferImageCopy region = {
        .bufferOffset                    = 0,
        .bufferRowLength                 = 0,
        .bufferImageHeight               = 0,
        .imageSubresource.aspectMask     = VK_IMAGE_ASPECT_COLOR_BIT,
        .imageSubresource.mipLevel       = ctx->vulkan_frame->mip_levels - 1,
        .imageSubresource.baseArrayLayer = 0,
        .imageSubresource.layerCount     = 1,
        .imageOffset                     = { 0, 0, 0 },
        .imageExtent                     = { 1, 1, 1 },
    };

    vkCmdCopyImageToBuffer(ctx->vulkan->command_buffer, ctx->vulkan_frame->image, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, ctx->vulkan->buffer, 1, &region);

    if (vkEndCommandBuffer(ctx->vulkan->command_buffer) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to end Vulkan command buffer!\n");
        goto exit;
    }

    VkSubmitInfo submitInfo = {
        .sType              = VK_STRUCTURE_TYPE_SUBMIT_INFO,
        .commandBufferCount = 1,
        .pCommandBuffers    = &ctx->vulkan->command_buffer,
    };

    if (vkQueueSubmit(ctx->vulkan->queue, 1, &submitInfo, ctx->vulkan->fence) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to submit Vulkan queue!\n");
        goto exit;
    }

    if (vkWaitForFences(ctx->vulkan->device, 1, &ctx->vulkan->fence, 1, VULKAN_FENCE_MAX_WAIT_NS) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to wait for Vulkan fence!\n");
        goto exit;
    }

    unsigned char* rgba;
    if (vkMapMemory(ctx->vulkan->device, ctx->vulkan->buffer_memory, 0, VK_WHOLE_SIZE, 0, (void *)&rgba) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to map Vulkan buffer memory!\n");
        goto exit;
    }

    unsigned char r = rgba[0], g = rgba[1], b = rgba[2];
    result = sqrt(0.241 * (double)(r * r) + 0.691 * (double)(g * g) + 0.068 * (double)(b * b)) / 255.0 * 100.0;

    vkUnmapMemory(ctx->vulkan->device, ctx->vulkan->buffer_memory);

    if (vkResetFences(ctx->vulkan->device, 1, &ctx->vulkan->fence) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to reset Vulkan fence!\n");
        goto exit;
    }

exit:
    if (frameImage)       vkDestroyImage(ctx->vulkan->device, frameImage, NULL);
    if (frameImageMemory) vkFreeMemory(ctx->vulkan->device, frameImageMemory, NULL);

    return result;
}


/******************************************************************************
 * Backlight control
 */

static void update_backlight(struct Context *ctx, long lux, int luma, int backlight) {
    if (!ctx->data || ctx->backlight_last != backlight) {
        struct DataPoint *new_point = data_add(ctx, lux, luma, backlight);
        struct DataPoint *elem = ctx->data;
        while (elem) {
            if (
                (elem->lux == lux && elem->luma == luma && elem != new_point) ||
                (elem->lux >  lux && elem->luma == luma) ||
                (elem->lux <  lux && elem->luma >= luma && elem->backlight > backlight) ||
                (elem->lux == lux && elem->luma <  luma && elem->backlight < backlight) ||
                (elem->lux >  lux && elem->luma <= luma && elem->backlight < backlight) ||
                (elem->lux == lux && elem->luma >  luma && elem->backlight > backlight)
            ) {
                elem = data_remove(ctx, elem);
            } else {
                elem = elem->next;
            }
        }

        data_save(ctx);

        ctx->lux_max_seen = fmax(fmax(ctx->lux_max_seen, lux), 1);
    } else {
        struct DataPoint *nearest = ctx->data, *elem = ctx->data;
        long lux_capped = fmin(lux, ctx->lux_max_seen);
        double nearest_dist = sqrt(pow((lux_capped - nearest->lux) * 100 / ctx->lux_max_seen, 2) + pow(luma - nearest->luma, 2));

        while (elem) {
            double dist = sqrt(pow((lux_capped - elem->lux) * 100 / ctx->lux_max_seen, 2) + pow(luma - elem->luma, 2));
            if (dist < nearest_dist) {
                nearest_dist = dist;
                nearest = elem;
            }
            elem = elem->next;
        }

        if (backlight != nearest->backlight) {
            struct timespec sleep = { 0 };
            for (
                int step = backlight < nearest->backlight ? 1 : -1;
                (step > 0 && backlight <= nearest->backlight) || (step < 0 && backlight >= nearest->backlight);
                backlight += step
            ) {
                pwrite_long(ctx->backlight_raw_fd, backlight * ctx->backlight_max / 100);

                sleep.tv_nsec = BACKLIGHT_TRANSITION_DELAY_NS;
                while (nanosleep(&sleep, &sleep) == -1) {
                    continue;
                }
            }
            backlight = nearest->backlight;
        }
    }

    ctx->backlight_last = backlight;
}


/******************************************************************************
 * Frame management
 */
static void register_frame_listener(struct Context *ctx);

static void frame_free(struct Context *ctx) {
    if (ctx->frame == NULL) {
        return;
    }

    zwlr_export_dmabuf_frame_v1_destroy(ctx->frame->frame);

    for (uint32_t i = 0; i < ctx->frame->num_objects; i++) {
        close(ctx->frame->fds[i]);
    }

    free(ctx->frame);
    ctx->frame = NULL;
}

static void frame_ready(void *data, struct zwlr_export_dmabuf_frame_v1 *frame,
                        uint32_t tv_sec_hi, uint32_t tv_sec_lo, uint32_t tv_nsec) {
    struct Context *ctx = data;

    // Compute all necessary values
    int luma = compute_frame_luma_pct(ctx);
    frame_free(ctx);

    long lux = read_lux(ctx);
    int backlight = read_backlight_pct(ctx);

    // Don't update backlight if there was an error or exit signal
    if (ctx->quit || ctx->err) {
        return;
    }

    // Set the most appropriate backlight value
    update_backlight(ctx, lux, luma, backlight);

    // Sleep a bit before asking for the next frame
    struct timespec sleep = { .tv_nsec = FRAME_REQUEST_DELAY_NS };
    while (nanosleep(&sleep, &sleep) == -1) {
        continue;
    }

    // Ask for the next frame
    register_frame_listener(ctx);
}

static void frame_start(void *data, struct zwlr_export_dmabuf_frame_v1 *frame,
        uint32_t width, uint32_t height, uint32_t offset_x, uint32_t offset_y,
        uint32_t buffer_flags, uint32_t flags, uint32_t format,
        uint32_t mod_high, uint32_t mod_low, uint32_t num_objects) {
    struct Context *ctx = data;

    ctx->frame = malloc(sizeof(struct Frame));
    ctx->frame->frame = frame;
    ctx->frame->width = width;
    ctx->frame->height = height;
    ctx->frame->num_objects = num_objects;

    init_frame_vulkan(ctx);
}

static void frame_object(void *data, struct zwlr_export_dmabuf_frame_v1 *frame,
        uint32_t index, int32_t fd, uint32_t size, uint32_t offset,
        uint32_t stride, uint32_t plane_index) {
    struct Context *ctx = data;

    ctx->frame->fds[index] = fd;
    ctx->frame->sizes[index] = size;
}

static void frame_cancel(void *data, struct zwlr_export_dmabuf_frame_v1 *frame,
        uint32_t reason) {
    struct Context *ctx = data;

    frame_free(ctx);

    if (reason == ZWLR_EXPORT_DMABUF_FRAME_V1_CANCEL_REASON_PERMANENT) {
        fprintf(stderr, "ERROR: Permanent failure when capturing frame!\n");
        ctx->err = 1;
    } else {
        register_frame_listener(ctx);
    }
}

static const struct zwlr_export_dmabuf_frame_v1_listener frame_listener = {
    .frame  = frame_start,
    .object = frame_object,
    .ready  = frame_ready,
    .cancel = frame_cancel,
};

static void register_frame_listener(struct Context *ctx) {
    ctx->frame_callback = zwlr_export_dmabuf_manager_v1_capture_output(ctx->dmabuf_manager, false, ctx->target_output->output);
    zwlr_export_dmabuf_frame_v1_add_listener(ctx->frame_callback, &frame_listener, ctx);
}


/******************************************************************************
 * Outputs management
 */
static void remove_output(struct WaylandOutput *out) {
    wl_list_remove(&out->link);
}

static struct WaylandOutput* find_output(struct Context *ctx, struct wl_output *out, uint32_t id) {
    struct WaylandOutput *output, *tmp;
    wl_list_for_each_safe(output, tmp, ctx->outputs, link) {
        if ((output->output == out) || (output->id == id)) {
            return output;
        }
    }
    return NULL;
}

static void registry_handle_remove(void *data, struct wl_registry *reg, uint32_t id) {
    remove_output(find_output((struct Context*)data, NULL, id));
}

static void registry_handle_add(void *data, struct wl_registry *reg, uint32_t id, const char *interface, uint32_t ver) {
    struct Context *ctx = data;

    if (strcmp(interface, wl_output_interface.name) == 0) {
        struct WaylandOutput *output = malloc(sizeof(struct WaylandOutput));

        output->id = id;
        output->output = wl_registry_bind(reg, id, &wl_output_interface, ver);

        wl_list_insert(ctx->outputs, &output->link);
    }

    if (strcmp(interface, zwlr_export_dmabuf_manager_v1_interface.name) == 0) {
        ctx->dmabuf_manager = wl_registry_bind(reg, id, &zwlr_export_dmabuf_manager_v1_interface, ver);
    }
}


/******************************************************************************
 * Main loop
 */
struct Context *quit_ctx = NULL;

static void on_quit_signal(int signal) {
    printf("\r");
    quit_ctx->quit = true;
}

static int main_loop(struct Context *ctx) {
    quit_ctx = ctx;

    if (signal(SIGINT, on_quit_signal) == SIG_ERR) {
        fprintf(stderr, "ERROR: Failed to install signal handler!\n");
        return EXIT_FAILURE;
    }

    register_frame_listener(ctx);

    // Run capture
    while (wl_display_dispatch(ctx->display) != -1 && !ctx->err && !ctx->quit);

    return ctx->err;
}


/******************************************************************************
 * Initialize Wayland client and Vulkan API
 */
static int init(struct Context *ctx, int argc, char *argv[]) {
    char buf[1024];
    char *backlight_raw_name = get_env("WLUMA_BACKLIGHT_NAME", "intel_backlight");
    char *light_sensor_raw_base_path = get_env("WLUMA_LIGHT_SENSOR_BASE_PATH", "/sys/bus/iio/devices");

    sprintf(buf, "/sys/class/backlight/%s/max_brightness", backlight_raw_name);
    int fd = open(buf, O_RDONLY);
    if (fd < 1) {
        fprintf(stderr, "ERROR: Failed to open max_brightness file: %s\n", buf);
        return EXIT_FAILURE;
    }
    ctx->backlight_max = pread_double(fd);
    close(fd);

    sprintf(buf, "/sys/class/backlight/%s/brightness", backlight_raw_name);
    ctx->backlight_raw_fd = open(buf, O_RDWR);
    if (ctx->backlight_raw_fd == -1) {
        fprintf(stderr, "ERROR: Failed to open brightness device file: %s\n", buf);
        return EXIT_FAILURE;
    }
    ctx->backlight_last = read_backlight_pct(ctx);

    DIR *dir = opendir(light_sensor_raw_base_path);
    if (dir == NULL) {
        fprintf(stderr, "ERROR: Failed to open light sensor base dir: %s\n", light_sensor_raw_base_path);
        return EXIT_FAILURE;
    }

    struct dirent *subdir;
    while ((subdir = readdir(dir))) {
        if (subdir->d_name[0] == '.') {
            continue;
        }

        sprintf(buf, "%s/%s/name", light_sensor_raw_base_path, subdir->d_name);
        fd = open(buf, O_RDONLY);
        if (fd > 0) {
            int count = fmax(1, read(fd, buf, sizeof(buf)));
            buf[count] = 0;
            buf[strcspn(buf, "\n")] = 0;
            close(fd);

            if (!strcmp("als", buf)) {
                ctx->light_sensor_scale = 1;
                sprintf(buf, "%s/%s/in_illuminance_scale", light_sensor_raw_base_path, subdir->d_name);
                fd = open(buf, O_RDONLY);
                if (fd > 0) {
                    ctx->light_sensor_scale = pread_double(fd);
                    close(fd);
                }

                ctx->light_sensor_offset = 0;
                sprintf(buf, "%s/%s/in_illuminance_offset", light_sensor_raw_base_path, subdir->d_name);
                fd = open(buf, O_RDONLY);
                if (fd > 0) {
                    ctx->light_sensor_offset = pread_double(fd);
                    close(fd);
                }

                sprintf(buf, "%s/%s/in_illuminance_raw", light_sensor_raw_base_path, subdir->d_name);
                ctx->light_sensor_raw_fd = open(buf, O_RDONLY);
                if (ctx->light_sensor_raw_fd > 0) {
                    break;
                }
            }
        }
    }
    closedir(dir);

    if (ctx->light_sensor_raw_fd < 1) {
        fprintf(stderr, "ERROR: Failed to find ambient light sensor device file in base dir: %s\n", light_sensor_raw_base_path);
        return EXIT_FAILURE;
    }

    char *data_dir = get_env("XDG_DATA_HOME", NULL);
    if (data_dir == NULL) {
        data_dir = get_env("HOME", NULL);
        if (data_dir == NULL) {
            fprintf(stderr, "ERROR: Failed to read $XDG_DATA_HOME or $HOME!\n");
            return EXIT_FAILURE;
        }

        sprintf(buf, "%s/.local/share/wluma", data_dir);
    } else {
        sprintf(buf, "%s/wluma", data_dir);
    }
    mkdir(buf, 0700);

    strcat(buf, "/data");
    ctx->data_fd = open(buf, O_RDWR | O_CREAT | O_DSYNC, 0600);
    if (ctx->data_fd == -1) {
        fprintf(stderr, "ERROR: Failed to open data file!\n");
        return EXIT_FAILURE;
    }

    if (!data_load(ctx)) {
        fprintf(stderr, "WARN: Failed to read data file, starting from scratch!\n");
    }

    ctx->display = wl_display_connect(NULL);
    if (!ctx->display) {
        fprintf(stderr, "ERROR: Failed to connect to display!\n");
        return EXIT_FAILURE;
    }

    ctx->outputs = malloc(sizeof(struct wl_list));
    wl_list_init(ctx->outputs);

    struct wl_registry *registry = wl_display_get_registry(ctx->display);

    struct wl_registry_listener listener = {
        .global = registry_handle_add,
        .global_remove = registry_handle_remove,
    };
    wl_registry_add_listener(registry, &listener, ctx);

    wl_display_roundtrip(ctx->display);
    wl_display_dispatch(ctx->display);

    if (wl_list_empty(ctx->outputs)) {
        fprintf(stderr, "ERROR: Failed to retrieve any output!\n");
        return EXIT_FAILURE;
    }

    if (!ctx->dmabuf_manager) {
        fprintf(stderr, "ERROR: Failed to initialize DMA-BUF manager!\n");
        return EXIT_FAILURE;
    }

    ctx->vulkan = malloc(sizeof(struct Vulkan));

    VkApplicationInfo appInfo = {
        .sType              = VK_STRUCTURE_TYPE_APPLICATION_INFO,
        .pApplicationName   = "wluma",
        .applicationVersion = VK_MAKE_VERSION(1, 0, 0),
        .pEngineName        = "No Engine",
        .engineVersion      = VK_MAKE_VERSION(1, 0, 0),
        .apiVersion         = VK_API_VERSION_1_0,
    };

    VkInstanceCreateInfo instanceCreateInfo = {
        .sType             = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
        .pApplicationInfo  = &appInfo,
    };

    if (vkCreateInstance(&instanceCreateInfo, NULL, &ctx->vulkan->instance) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to initialize Vulkan instance!\n");
        return EXIT_FAILURE;
    }

    VkPhysicalDevice physicalDevice;
    uint32_t deviceCount;
    if (vkEnumeratePhysicalDevices(ctx->vulkan->instance, &deviceCount, NULL) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to retrieve Vulkan physical device!\n");
        return EXIT_FAILURE;
    }

    if (deviceCount == 0) {
        fprintf(stderr, "ERROR: No physical device that supports Vulkan!\n");
        return EXIT_FAILURE;
    }

    VkPhysicalDevice *physicalDevices = calloc(deviceCount, sizeof(VkPhysicalDevice));
    if (vkEnumeratePhysicalDevices(ctx->vulkan->instance, &deviceCount, physicalDevices) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to retrieve Vulkan physical device!\n");
        return EXIT_FAILURE;
    }
    // TODO handle multiple physical devices
    physicalDevice = physicalDevices[0];
    free(physicalDevices);

    float queuePriority = 1.0f;
    VkDeviceQueueCreateInfo queueCreateInfo = {
        .sType            = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
        .queueFamilyIndex = 0,
        .queueCount       = 1,
        .pQueuePriorities = &queuePriority,
    };

    VkDeviceCreateInfo deviceCreateInfo = {
        .sType                = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
        .pQueueCreateInfos    = &queueCreateInfo,
        .queueCreateInfoCount = 1,
    };

    if (vkCreateDevice(physicalDevice, &deviceCreateInfo, NULL, &ctx->vulkan->device) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to initialize Vulkan logical device!\n");
        return EXIT_FAILURE;
    }

    vkGetDeviceQueue(ctx->vulkan->device, 0, 0, &ctx->vulkan->queue);

    VkCommandPoolCreateInfo poolInfo = {
        .sType            = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
        .queueFamilyIndex = 0,
        .flags            = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT,
    };

    if (vkCreateCommandPool(ctx->vulkan->device, &poolInfo, NULL, &ctx->vulkan->command_pool) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to create Vulkan command pool!\n");
        return EXIT_FAILURE;
    }

    VkCommandBufferAllocateInfo cmdBufferAllocInfo = {
        .sType              = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
        .level              = VK_COMMAND_BUFFER_LEVEL_PRIMARY,
        .commandPool        = ctx->vulkan->command_pool,
        .commandBufferCount = 1,
    };

    if (vkAllocateCommandBuffers(ctx->vulkan->device, &cmdBufferAllocInfo, &ctx->vulkan->command_buffer) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to allocate Vulkan command buffer!\n");
        return EXIT_FAILURE;
    }

    VkBufferCreateInfo bufferInfo = {
        .sType       = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        .size        = 4, // 1 byte per RGBA
        .usage       = VK_BUFFER_USAGE_TRANSFER_DST_BIT,
        .sharingMode = VK_SHARING_MODE_EXCLUSIVE,
    };

    if (vkCreateBuffer(ctx->vulkan->device, &bufferInfo, NULL, &ctx->vulkan->buffer) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to create Vulkan buffer!\n");
        return EXIT_FAILURE;
    }

    VkMemoryRequirements bufferMemoryRequirements;
    vkGetBufferMemoryRequirements(ctx->vulkan->device, ctx->vulkan->buffer, &bufferMemoryRequirements);

    VkMemoryAllocateInfo bufferMemoryAllocateInfo ={
        .sType           = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize  = bufferMemoryRequirements.size,
        .memoryTypeIndex = 0,
    };

    if (vkAllocateMemory(ctx->vulkan->device, &bufferMemoryAllocateInfo, NULL, &ctx->vulkan->buffer_memory) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to allocate memory for Vulkan buffer!\n");
        return EXIT_FAILURE;
    }

    if (vkBindBufferMemory(ctx->vulkan->device, ctx->vulkan->buffer, ctx->vulkan->buffer_memory, 0) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to bind allocated memory for Vulkan buffer!\n");
        return EXIT_FAILURE;
    }

    VkFenceCreateInfo fenceInfo = {
        .sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO,
    };

    if (vkCreateFence(ctx->vulkan->device, &fenceInfo, NULL, &ctx->vulkan->fence) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to create Vulkan fence!\n");
        return EXIT_FAILURE;
    }

    return EXIT_SUCCESS;
}

static void deinit(struct Context *ctx) {
    struct DataPoint *elem = ctx->data;
    while (elem) {
        elem = data_remove(ctx, elem);
    }

    if (ctx->outputs) {
        struct WaylandOutput *output, *tmp_o;
        wl_list_for_each_safe(output, tmp_o, ctx->outputs, link) {
            remove_output(output);
        }

        free(ctx->outputs);
    }

    if (ctx->dmabuf_manager) zwlr_export_dmabuf_manager_v1_destroy(ctx->dmabuf_manager);

    if (ctx->vulkan_frame) {
        if (ctx->vulkan_frame->image)        vkDestroyImage(ctx->vulkan->device, ctx->vulkan_frame->image, NULL);
        if (ctx->vulkan_frame->image_memory) vkFreeMemory(ctx->vulkan->device, ctx->vulkan_frame->image_memory, NULL);

        free(ctx->vulkan_frame);
    }

    if (ctx->vulkan_frame) {
        if (ctx->vulkan->fence)          vkDestroyFence(ctx->vulkan->device, ctx->vulkan->fence, NULL);
        if (ctx->vulkan->buffer)         vkDestroyBuffer(ctx->vulkan->device, ctx->vulkan->buffer, NULL);
        if (ctx->vulkan->buffer_memory)  vkFreeMemory(ctx->vulkan->device, ctx->vulkan->buffer_memory, NULL);
        if (ctx->vulkan->command_buffer) vkFreeCommandBuffers(ctx->vulkan->device, ctx->vulkan->command_pool, 1, &ctx->vulkan->command_buffer);
        if (ctx->vulkan->command_pool)   vkDestroyCommandPool(ctx->vulkan->device, ctx->vulkan->command_pool, NULL);
        if (ctx->vulkan->device)         vkDestroyDevice(ctx->vulkan->device, NULL);
        if (ctx->vulkan->instance)       vkDestroyInstance(ctx->vulkan->instance, NULL);

        free(ctx->vulkan);
    }

    close(ctx->data_fd);
    close(ctx->backlight_raw_fd);
    close(ctx->light_sensor_raw_fd);
}


/******************************************************************************
 * Main
 */

int main(int argc, char *argv[]) {
    int err = EXIT_SUCCESS;
    struct Context ctx = { 0 };

    err = init(&ctx, argc, argv);
    if (err) {
        goto exit;
    }

    // TODO handle multiple outputs
    struct WaylandOutput *o, *tmp_o;
    wl_list_for_each_safe(o, tmp_o, ctx.outputs, link) {
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
