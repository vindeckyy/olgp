// SPDX-License-Identifier: GPL-2.0-only
#include "olgp_drm.h"

#include <linux/uaccess.h>

static struct olgp_drm *to_olgp(struct drm_device *drm)
{
	return drm->dev_private;
}

int olgp_ioctl_submit(struct drm_device *drm, void *data, struct drm_file *file)
{
	struct olgp_drm *olgp = to_olgp(drm);
	struct olgp_submit request;
	unsigned int next;

	(void)file;
	if (copy_from_user(&request, data, sizeof(request)))
		return -EFAULT;
	if (request.payload_len > 4096)
		return -EINVAL;

	mutex_lock(&olgp->queue_lock);
	next = (olgp->tail + 1) % OLGP_QUEUE_SIZE;
	if (next == olgp->head) {
		mutex_unlock(&olgp->queue_lock);
		return -EBUSY;
	}
	olgp->queue[olgp->tail] = request;
	olgp->tail = next;
	mutex_unlock(&olgp->queue_lock);

	wake_up_interruptible(&olgp->queue_wq);
	return 0;
}

int olgp_ioctl_recv(struct drm_device *drm, void *data, struct drm_file *file)
{
	struct olgp_drm *olgp = to_olgp(drm);
	struct olgp_response response;
	struct olgp_submit request;
	unsigned int flags;

	(void)file;
	wait_event_interruptible(olgp->queue_wq, olgp->head != olgp->tail);

	mutex_lock(&olgp->queue_lock);
	if (olgp->head == olgp->tail) {
		mutex_unlock(&olgp->queue_lock);
		return -ERESTARTSYS;
	}
	request = olgp->queue[olgp->head];
	olgp->head = (olgp->head + 1) % OLGP_QUEUE_SIZE;
	flags = request.flags;
	mutex_unlock(&olgp->queue_lock);

	memset(&response, 0, sizeof(response));
	response.request_id = request.request_id;
	response.status = request.opcode;
	response.reserved = flags;
	if (copy_to_user(data, &response, sizeof(response)))
		return -EFAULT;
	return 0;
}
