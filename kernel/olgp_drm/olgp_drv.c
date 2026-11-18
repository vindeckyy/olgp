// SPDX-License-Identifier: GPL-2.0-only
#include "olgp_drm.h"

#include <linux/module.h>
#include <linux/platform_device.h>
#include <linux/pci.h>
#include <drm/drm_drv.h>
#include <drm/drm_file.h>
#include <drm/drm_gem.h>
#include <drm/drm_gem_shmem_helper.h>
#include <drm/drm_ioctl.h>

#define OLGP_DRV_SUBMIT 0x40
#define OLGP_DRV_RECV   0x41

static long olgp_file_ioctl(struct file *filp, unsigned int cmd, unsigned long arg)
{
	struct drm_file *file = filp->private_data;
	struct drm_device *drm = file->minor->dev;

	switch (_IOC_NR(cmd)) {
	case OLGP_DRV_SUBMIT:
		return olgp_ioctl_submit(drm, (void __user *)arg, file);
	case OLGP_DRV_RECV:
		return olgp_ioctl_recv(drm, (void __user *)arg, file);
	default:
		return drm_ioctl(filp, cmd, arg);
	}
}

static const struct file_operations olgp_fops = {
	.owner = THIS_MODULE,
	.open = drm_open,
	.release = drm_release,
	.unlocked_ioctl = olgp_file_ioctl,
	.compat_ioctl = olgp_file_ioctl,
	.poll = drm_poll,
	.read = drm_read,
	.mmap = drm_gem_mmap,
	.llseek = noop_llseek,
};

static struct drm_driver olgp_driver = {
	.driver_features = DRIVER_GEM,
	.fops = &olgp_fops,
	.gem_create_object = olgp_gem_create_object,
	.gem_free_object_unlocked = drm_gem_shmem_free,
	.name = "olgp",
	.desc = "Open Local GPU Protocol virtual DRM device",
	.date = "2026-11-17",
	.major = 0,
	.minor = 1,
	.patchlevel = 0,
};

static int olgp_drm_probe_common(struct device *dev)
{
	struct olgp_drm *olgp;
	struct drm_device *drm;
	int ret;

	olgp = devm_kzalloc(dev, sizeof(*olgp), GFP_KERNEL);
	if (!olgp)
		return -ENOMEM;

	mutex_init(&olgp->queue_lock);
	init_waitqueue_head(&olgp->queue_wq);
	olgp->dev = dev;

	drm = drm_dev_alloc(&olgp_driver, dev);
	if (IS_ERR(drm))
		return PTR_ERR(drm);
	drm->dev_private = olgp;
	olgp->drm = drm;

	ret = drm_dev_register(drm, 0);
	if (ret) {
		drm_dev_put(drm);
		return ret;
	}
	dev_set_drvdata(dev, olgp);
	return 0;
}

static int olgp_drm_remove_common(struct device *dev)
{
	struct olgp_drm *olgp = dev_get_drvdata(dev);

	if (!olgp)
		return 0;
	drm_dev_unregister(olgp->drm);
	drm_dev_put(olgp->drm);
	return 0;
}

static const struct platform_device_id olgp_platform_ids[] = {
	{ .name = "olgp-drm" },
	{ }
};
MODULE_DEVICE_TABLE(platform, olgp_platform_ids);

static int olgp_platform_probe(struct platform_device *pdev)
{
	return olgp_drm_probe_common(&pdev->dev);
}

static int olgp_platform_remove(struct platform_device *pdev)
{
	return olgp_drm_remove_common(&pdev->dev);
}

static struct platform_driver olgp_platform_driver = {
	.probe = olgp_platform_probe,
	.remove = olgp_platform_remove,
	.id_table = olgp_platform_ids,
	.driver = { .name = "olgp-drm" },
};

static const struct pci_device_id olgp_pci_ids[] = {
	{ PCI_DEVICE(OLGP_PCI_VENDOR, OLGP_PCI_DEVICE) },
	{ 0 }
};
MODULE_DEVICE_TABLE(pci, olgp_pci_ids);

static int olgp_pci_probe(struct pci_dev *pdev, const struct pci_device_id *id)
{
	int ret;

	(void)id;
	ret = pci_enable_device(pdev);
	if (ret)
		return ret;
	return olgp_drm_probe_common(&pdev->dev);
}

static void olgp_pci_remove(struct pci_dev *pdev)
{
	olgp_drm_remove_common(&pdev->dev);
	pci_disable_device(pdev);
}

static struct pci_driver olgp_pci_driver = {
	.name = "olgp-drm-pci",
	.id_table = olgp_pci_ids,
	.probe = olgp_pci_probe,
	.remove = olgp_pci_remove,
};

static int __init olgp_drm_init(void)
{
	int ret;

	ret = platform_driver_register(&olgp_platform_driver);
	if (ret)
		return ret;
	ret = pci_register_driver(&olgp_pci_driver);
	if (ret) {
		platform_driver_unregister(&olgp_platform_driver);
		return ret;
	}
	return 0;
}

static void __exit olgp_drm_exit(void)
{
	pci_unregister_driver(&olgp_pci_driver);
	platform_driver_unregister(&olgp_platform_driver);
}

module_init(olgp_drm_init);
module_exit(olgp_drm_exit);

MODULE_LICENSE("GPL");
MODULE_AUTHOR("OLGP contributors");
MODULE_DESCRIPTION("Open Local GPU Protocol virtual DRM device");
