# SPDX-License-Identifier: GPL-2.0-only
ifneq ($(KERNELRELEASE),)
obj-m += olgp_drm.o
olgp_drm-objs := olgp_drv.o olgp_ioctl.o olgp_gem.o
else
.PHONY: default tool clean
.DEFAULT_GOAL := default

KDIR ?= /lib/modules/$(shell uname -r)/build
PWD := $(shell pwd)

default:
	$(MAKE) -C $(KDIR) M=$(PWD) modules

olgp_drm_tool: olgp_drm_tool.c olgp_abi.h
	$(CC) -Wall -Wextra -O2 -I. -o $@ $<

tool: olgp_drm_tool

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) clean
	rm -f olgp_drm_tool
endif
