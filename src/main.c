#define _POSIX_C_SOURCE 200809L

#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <time.h>
#include <vulkan/vulkan.h>
#include <wayland-client.h>

#include "wlr-export-dmabuf-unstable-v1-client-protocol.h"

#define MS_100 (100 * 1000000L)

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
    struct wl_list outputs;
    struct zwlr_export_dmabuf_manager_v1 *dmabuf_manager;

    // Target
    struct WaylandOutput *target_output;
    bool with_cursor;

    // Main frame callback
    struct zwlr_export_dmabuf_frame_v1 *frame_callback;

    // Vulkan context
    struct Vulkan *vulkan;

    // DMA-BUF frame
    struct Frame *frame;

    // Vulkan structs for processing frames, might be reused
    struct VulkanFrame *vulkan_frame;

    // Errors
    bool quit;
    int err;
};


/******************************************************************************
 * Vulkan
 */

static void init_frame_vulkan(struct Context *ctx) {
    if (ctx->vulkan_frame) {
        // TODO support resized frames
        return;
    }

    ctx->vulkan_frame = malloc(sizeof(struct VulkanFrame));

    ctx->vulkan_frame->mip_levels = 1 + floor(log2(fmax(ctx->frame->width, ctx->frame->height)));

    VkImageCreateInfo imageInfo = {
        .sType         = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
        .imageType     = VK_IMAGE_TYPE_2D,
        .format        = VK_FORMAT_B8G8R8A8_UNORM,
        .extent.width  = ctx->frame->width,
        .extent.height = ctx->frame->height,
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
        .dstOffsets[1]                 = { ctx->frame->width, ctx->frame->height, 1 },
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
    uint32_t mipWidth  = ctx->frame->width;
    uint32_t mipHeight = ctx->frame->height;

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

    if (vkWaitForFences(ctx->vulkan->device, 1, &ctx->vulkan->fence, 1, MS_100) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to wait for Vulkan fence!\n");
        goto exit;
    }

    unsigned char* rgba;
    if (vkMapMemory(ctx->vulkan->device, ctx->vulkan->buffer_memory, 0, VK_WHOLE_SIZE, 0, (void *)&rgba) != VK_SUCCESS) {
        fprintf(stderr, "ERROR: Failed to map Vulkan buffer memory!\n");
        goto exit;
    }

    unsigned char r = rgba[0], g = rgba[1], b = rgba[2];
    result = sqrt(0.241 * r * r + 0.691 * g * g + 0.068 * b * b) / 255.0 * 100.0;

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

    int luma = compute_frame_luma_pct(ctx);
    printf("luma: %d%%\n", luma);

    frame_free(ctx);

    if (!ctx->quit && !ctx->err) {
        // Sleep a bit before asking for the next frame
        struct timespec ts = {
            .tv_sec = 0,
            .tv_nsec = MS_100,
        };
        while (nanosleep(&ts, &ts) == -1) {
            continue;
        }

        // Ask for the next frame
        register_frame_listener(ctx);
    }
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
    ctx->frame->format = format;
    ctx->frame->format_modifier = ((uint64_t)mod_high << 32) | mod_low;
    ctx->frame->num_objects = num_objects;
    ctx->frame->flags = flags;

    init_frame_vulkan(ctx);
}

static void frame_object(void *data, struct zwlr_export_dmabuf_frame_v1 *frame,
        uint32_t index, int32_t fd, uint32_t size, uint32_t offset,
        uint32_t stride, uint32_t plane_index) {
    struct Context *ctx = data;

    ctx->frame->fds[index] = fd;
    ctx->frame->sizes[index] = size;
    ctx->frame->strides[index] = stride;
    ctx->frame->offsets[index] = offset;
    ctx->frame->plane_indices[index] = plane_index;
}

static void frame_cancel(void *data, struct zwlr_export_dmabuf_frame_v1 *frame,
        uint32_t reason) {
    struct Context *ctx = data;

    frame_free(ctx);

    if (reason == ZWLR_EXPORT_DMABUF_FRAME_V1_CANCEL_REASON_PERMANENT) {
        fprintf(stderr, "ERROR: Permanent failure when capturing frame!\n");
        ctx->err = true;
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
    ctx->frame_callback = zwlr_export_dmabuf_manager_v1_capture_output(ctx->dmabuf_manager, ctx->with_cursor, ctx->target_output->output);
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
    wl_list_for_each_safe(output, tmp, &ctx->outputs, link) {
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

        wl_list_insert(&ctx->outputs, &output->link);
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
    int err;

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
static int init(struct Context *ctx) {
    ctx->display = wl_display_connect(NULL);
    if (!ctx->display) {
        fprintf(stderr, "ERROR: Failed to connect to display!\n");
        return EXIT_FAILURE;
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
    struct WaylandOutput *output, *tmp_o;
    wl_list_for_each_safe(output, tmp_o, &ctx->outputs, link) {
        remove_output(output);
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
}


/******************************************************************************
 * Main
 */

int main() {
    int err = EXIT_SUCCESS;
    struct Context ctx = { 0 };

    err = init(&ctx);
    if (err) {
        goto exit;
    }

    // TODO handle multiple outputs
    struct WaylandOutput *o, *tmp_o;
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
