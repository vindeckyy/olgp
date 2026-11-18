# virtio-gpu Venus virtual GPU

This path exposes a real virtio-gpu Vulkan device to a Linux guest. It is a
literal virtual GPU: the guest enumerates a `VkPhysicalDevice` through Mesa's
Venus driver. It is separate from the CUDA/NVML shim path and does not provide
CUDA compatibility.

## Host requirements

- Linux host with KVM access.
- QEMU built with virglrenderer support.
- virglrenderer 1.0.0 or later built with `-Dvenus=true`.
- Host Vulkan driver with Vulkan 1.1 and `VK_KHR_external_memory_fd`.
- Linux 6.13+ for Vulkan pass-through.

## Guest requirements

- Linux guest kernel 5.16+ with `CONFIG_DRM_VIRTIO_GPU`.
- Mesa 24.2+ with the Venus Vulkan driver.

## Build and run

```sh
scripts/build-virglrenderer.sh
# Edit virtual/venus.toml for the guest image and resource limits.
VK_DRIVER_FILES=/path/to/virtio_icd.x86_64.json \
  VN_DEBUG=vtest \
  ./build-virglrenderer/vtest/virgl_test_server --venus &
scripts/venus-vm.sh
```

Inside the guest:

```sh
vulkaninfo --summary
vkcube
```

The generated QEMU device is:

```text
-device virtio-gpu-gl,hostmem=4096M,blob=true,venus=true
```

Use `olgpctl venus command --config virtual/venus.toml` to print the exact
QEMU command without starting it.
