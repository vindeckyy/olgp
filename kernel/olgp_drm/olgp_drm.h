/* SPDX-License-Identifier: GPL-2.0-only */
#ifndef OLGP_DRM_H
#define OLGP_DRM_H

#include "olgp_abi.h"

#include <linux/mutex.h>
#include <linux/wait.h>
#include <drm/drm_device.h>

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
