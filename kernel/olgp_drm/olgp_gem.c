// SPDX-License-Identifier: GPL-2.0-only
#include "olgp_drm.h"

#include <drm/drm_gem_shmem_helper.h>

struct drm_gem_object *olgp_gem_create_object(struct drm_device *drm, size_t size)
{
	struct olgp_gem_object *obj;

	obj = kzalloc(sizeof(*obj), GFP_KERNEL);
	if (!obj)
		return ERR_PTR(-ENOMEM);

	obj->base.map_wc = true;
	drm_gem_object_init(drm, &obj->base.base, size);
	return &obj->base.base;
}
