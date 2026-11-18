#ifndef OLGP_SEMANTIC_H
#define OLGP_SEMANTIC_H

#include <array>
#include <cstdint>
#include <mutex>
#include <string>
#include <unordered_map>
#include <vector>

namespace olgp {

// Semantic op codes used by the batch and trace engines. These are independent
// of LUPINE's generated RPC IDs; map_lupine_op translates between them.
enum class Op : uint16_t {
  None = 0,
  MemcpyHtoDAsync = 1,
  StreamSync = 2,
  ModuleLoadData = 3,
  CtxPush = 4,
  Launch = 5,
  MemcpyDtoH = 6,
  EventQuery = 7,
  StreamQuery = 8,
  Alloc = 9,
  Free = 10,
  CtxDestroy = 11,
};

Op map_lupine_op(int rpc_id);
bool is_safe_to_hold(Op op);
bool is_batch_pair(Op first, Op second);

struct BatchDecision {
  bool hold = false;
  std::vector<Op> batched;
};

class BatchRecognizer {
 public:
  BatchDecision note(Op op);
  uint64_t hits() const;
  uint64_t misses() const;
  uint64_t ops() const;

 private:
  Op held_ = Op::None;
  uint64_t hits_ = 0;
  uint64_t misses_ = 0;
  uint64_t ops_ = 0;
};

struct TraceEntry {
  Op op = Op::None;
  uint64_t template_hash = 0;
  bool operator==(const TraceEntry& other) const {
    return op == other.op && template_hash == other.template_hash;
  }
};

class TraceRecorder {
 public:
  void begin();
  void record(Op op, uint64_t template_hash);
  std::array<uint8_t, 16> finish();
  bool validate(const std::vector<TraceEntry>& entries) const;
  static bool safe(Op op);
  size_t size() const;

 private:
  std::vector<TraceEntry> entries_;
};

enum class HandleClass : uint8_t {
  ImmutableWeight = 1,
  KvCache = 2,
  Activation = 3,
  Framebuffer = 4,
};

struct HandleRecord {
  uint64_t id = 0;
  HandleClass handle_class = HandleClass::Activation;
  std::array<uint8_t, 32> content_hash{};
  uint32_t generation = 0;
  uint32_t refcount = 0;
};

class HandleTable {
 public:
  bool retain(uint64_t id, HandleClass handle_class, bool hash_verified,
              const std::array<uint8_t, 32>& content_hash);
  bool release(uint64_t id);
  bool may_share(uint64_t id) const;
  size_t size() const;

 private:
  mutable std::mutex mutex_;
  std::unordered_map<uint64_t, HandleRecord> handles_;
};

struct Counters {
  uint64_t batch_hits = 0;
  uint64_t batch_misses = 0;
  uint64_t batch_ops = 0;
  uint64_t trace_aborts = 0;
  uint64_t cache_hits = 0;
  uint64_t cache_misses = 0;
  uint64_t evictions = 0;
};

Counters& counters();
extern "C" uint64_t olgp_batch_hits();
extern "C" uint64_t olgp_batch_misses();
extern "C" uint64_t olgp_batch_ops();
extern "C" uint64_t olgp_trace_aborts();
void olgp_note_call(int rpc_id);

}  // namespace olgp

#endif
