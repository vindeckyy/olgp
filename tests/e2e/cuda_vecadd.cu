// CUDA vector-add workload used to verify the remote GPU shim.
// Build with: nvcc -o cuda_vecadd cuda_vecadd.cu
// The CUDA runtime calls the driver API underneath, so the shim intercepts it.
#include <cuda_runtime.h>
#include <cstdio>
#include <vector>

__global__ void add_kernel(const float *a, const float *b, float *out, unsigned n) {
  unsigned i = blockIdx.x * blockDim.x + threadIdx.x;
  if (i < n) out[i] = a[i] + b[i];
}

int main() {
  const unsigned count = 1u << 20;
  std::vector<float> a(count, 1.0f);
  std::vector<float> b(count, 2.0f);
  std::vector<float> out(count, 0.0f);
  float *da = nullptr;
  float *db = nullptr;
  float *dout = nullptr;
  const size_t bytes = static_cast<size_t>(count) * sizeof(float);

  if (cudaMalloc(&da, bytes) != cudaSuccess || cudaMalloc(&db, bytes) != cudaSuccess ||
      cudaMalloc(&dout, bytes) != cudaSuccess) {
    std::fprintf(stderr, "allocation failed\n");
    return 1;
  }
  cudaMemcpy(da, a.data(), bytes, cudaMemcpyHostToDevice);
  cudaMemcpy(db, b.data(), bytes, cudaMemcpyHostToDevice);
  add_kernel<<<(count + 255) / 256, 256>>>(da, db, dout, count);
  if (cudaDeviceSynchronize() != cudaSuccess) {
    std::fprintf(stderr, "synchronization failed\n");
    return 1;
  }
  cudaMemcpy(out.data(), dout, bytes, cudaMemcpyDeviceToHost);
  for (unsigned i = 0; i < count; ++i) {
    if (out[i] != 3.0f) {
      std::fprintf(stderr, "mismatch at %u\n", i);
      return 1;
    }
  }
  cudaFree(da);
  cudaFree(db);
  cudaFree(dout);
  std::printf("sum ok\n");
  return 0;
}
