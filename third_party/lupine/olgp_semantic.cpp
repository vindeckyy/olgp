#include "olgp_semantic.h"

#include "codegen/gen_rpc_ids.h"

#include <algorithm>
#include <cstring>
#include <functional>

#ifdef OLGP_SEMANTIC_OPENSSL
#include <openssl/sha.h>
#endif

namespace olgp {
namespace {

std::array<uint8_t, 16> digest16(const void* data, size_t length) {
  std::array<uint8_t, 16> out{};
#ifdef OLGP_SEMANTIC_OPENSSL
  unsigned char digest[SHA256_DIGEST_LENGTH];
  SHA256(static_cast<const unsigned char*>(data), length, digest);
  std::copy(digest, digest + out.size(), out.begin());
#else
  uint64_t hash = 1469598103934665603ULL;
  const auto* bytes = static_cast<const unsigned char*>(data);
  for (size_t i = 0; i < length; ++i) {
    hash ^= bytes[i];
    hash *= 1099511628211ULL;
  }
  for (size_t i = 0; i < out.size(); ++i) {
    out[i] = static_cast<uint8_t>(hash >> (i * 8));
  }
#endif
  return out;
}

}  // namespace

Op map_lupine_op(int rpc_id) {
  switch (rpc_id) {
    case RPC_cuMemcpyHtoDAsync_v2: return Op::MemcpyHtoDAsync;
    case RPC_cuStreamSynchronize: return Op::StreamSync;
    case RPC_cuModuleLoadData:
    case RPC_cuModuleLoadDataEx: return Op::ModuleLoadData;
    case RPC_cuCtxPushCurrent_v2: return Op::CtxPush;
    case RPC_cuLaunchKernel:
    case RPC_cuLaunchKernelEx: return Op::Launch;
    case RPC_cuMemcpyDtoH_v2: return Op::MemcpyDtoH;
    case RPC_cuEventQuery: return Op::EventQuery;
    case RPC_cuStreamQuery: return Op::StreamQuery;
    case RPC_cuMemAlloc_v2: return Op::Alloc;
    case RPC_cuMemFree_v2: return Op::Free;
    case RPC_cuCtxDestroy_v2: return Op::CtxDestroy;
    default: return Op::None;
  }
}

bool is_safe_to_hold(Op op) {
  return op == Op::MemcpyHtoDAsync || op == Op::ModuleLoadData || op == Op::Launch;
}

bool is_batch_pair(Op first, Op second) {
  return (first == Op::MemcpyHtoDAsync && second == Op::StreamSync) ||
         (first == Op::ModuleLoadData && second == Op::CtxPush) ||
         (first == Op::Launch && second == Op::Launch);
}

BatchDecision BatchRecognizer::note(Op op) {
  ++ops_;
  BatchDecision decision;
  if (held_ != Op::None) {
    if (is_batch_pair(held_, op)) {
      decision.hold = false;
      decision.batched = {held_, op};
      held_ = Op::None;
      ++hits_;
      return decision;
    }
    ++misses_;
    held_ = Op::None;
  }
  if (is_safe_to_hold(op)) {
    held_ = op;
    decision.hold = true;
  }
  return decision;
}

uint64_t BatchRecognizer::hits() const { return hits_; }
uint64_t BatchRecognizer::misses() const { return misses_; }
uint64_t BatchRecognizer::ops() const { return ops_; }

bool TraceRecorder::safe(Op op) {
  return op == Op::MemcpyHtoDAsync || op == Op::ModuleLoadData || op == Op::Launch;
}

void TraceRecorder::begin() { entries_.clear(); }

void TraceRecorder::record(Op op, uint64_t template_hash) {
  if (entries_.size() >= 512 || !safe(op)) {
    return;
  }
  entries_.push_back({op, template_hash});
}

std::array<uint8_t, 16> TraceRecorder::finish() {
  std::vector<uint8_t> bytes;
  bytes.reserve(entries_.size() * 10);
  for (const auto& entry : entries_) {
    const uint16_t op = static_cast<uint16_t>(entry.op);
    bytes.push_back(static_cast<uint8_t>(op));
    bytes.push_back(static_cast<uint8_t>(op >> 8));
    for (int i = 0; i < 8; ++i) {
      bytes.push_back(static_cast<uint8_t>(entry.template_hash >> (i * 8)));
    }
  }
  return digest16(bytes.data(), bytes.size());
}

bool TraceRecorder::validate(const std::vector<TraceEntry>& entries) const {
  return entries.size() == entries_.size() &&
         std::equal(entries.begin(), entries.end(), entries_.begin());
}

size_t TraceRecorder::size() const { return entries_.size(); }

bool HandleTable::retain(uint64_t id, HandleClass handle_class, bool hash_verified,
                         const std::array<uint8_t, 32>& content_hash) {
  std::lock_guard<std::mutex> lock(mutex_);
  if (handle_class == HandleClass::ImmutableWeight && !hash_verified) {
    return false;
  }
  auto& record = handles_[id];
  record.id = id;
  record.handle_class = handle_class;
  record.content_hash = content_hash;
  record.refcount += 1;
  return true;
}

bool HandleTable::release(uint64_t id) {
  std::lock_guard<std::mutex> lock(mutex_);
  auto it = handles_.find(id);
  if (it == handles_.end() || it->second.refcount == 0) {
    return false;
  }
  if (--it->second.refcount == 0) {
    handles_.erase(it);
  }
  return true;
}

bool HandleTable::may_share(uint64_t id) const {
  std::lock_guard<std::mutex> lock(mutex_);
  const auto it = handles_.find(id);
  return it != handles_.end() &&
         it->second.handle_class == HandleClass::ImmutableWeight &&
         it->second.refcount > 0;
}

size_t HandleTable::size() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return handles_.size();
}

Counters& counters() {
  static Counters value;
  return value;
}

extern "C" uint64_t olgp_batch_hits() { return counters().batch_hits; }
extern "C" uint64_t olgp_batch_misses() { return counters().batch_misses; }
extern "C" uint64_t olgp_batch_ops() { return counters().batch_ops; }
extern "C" uint64_t olgp_trace_aborts() { return counters().trace_aborts; }

void olgp_note_call(int rpc_id) {
  const Op op = map_lupine_op(rpc_id);
  if (op == Op::None) {
    return;
  }
  ++counters().batch_ops;
}

}  // namespace olgp
