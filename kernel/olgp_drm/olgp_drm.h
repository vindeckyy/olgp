/* SPDX-License-Identifier: GPL-2.0-only */
#ifndef OLGP_DRM_H
#define OLGP_DRM_H

#include <linux/types.h>
#include <linux/mutex.h>
#include <linux/wait.h>
#include <drm/drm_device.h>

#define OLGP_PCI_VENDOR 0x4f4c
#define OLGP_PCI_DEVICE 0x0001

struct olgp_submit {
	__u32 opcode;
	__u32 flags;
	__u64 payload_ptr;
	__u32 payload_len;
	__u32 request_id;
};

struct olgp_response {
	__u32 request_id;
	__u32 status;
	__u64 payload_ptr;
	__u32 payload_len;
	__u32 reserved;
};

#define OLGP_IOC_SUBMIT _IOW('O', 0x40, struct olgp_submit)
#define OLGP_IOC_RECV   _IOR('O', 0x41, struct olgp_response)

#define OLGP_QUEUE_SIZE 64

struct olgp_drm {
	struct drm_device *drm;
	struct device *dev;
	struct mutex queue_lock;
	wait_queue_head_t queue_wq;
	struct olgp_submit queue[OLGP_QUEUE_SIZE];
	unsigned int head;
	unsigned int tail;
};

struct olgp_gem_object {
	struct drm_gem_shmem_object base;
};

int olgp_ioctl_submit(struct drm_device *drm, void *data, struct drm_file *file);
int olgp_ioctl_recv(struct drm_device *drm, void *data, struct drm_file *file);
struct drm_gem_object *olgp_gem_create_object(struct drm_device *drm, size_t size);

#endif
