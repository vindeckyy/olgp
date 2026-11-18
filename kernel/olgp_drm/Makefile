# SPDX-License-Identifier: GPL-2.0-only
obj-m += olgp_drm.o

olgp_drm-objs := olgp_drv.o olgp_ioctl.o olgp_gem.o

KDIR ?= /lib/modules/$(shell uname -r)/build
PWD := $(shell pwd)

olgp_drm_tool: olgp_drm_tool.c olgp_drm.h
	$(CC) -Wall -Wextra -O2 -I. -o $@ $<

tool: olgp_drm_tool

default:
	$(MAKE) -C $(KDIR) M=$(PWD) modules

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean
