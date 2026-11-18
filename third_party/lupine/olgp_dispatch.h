#ifndef OLGP_DISPATCH_H
#define OLGP_DISPATCH_H

#include <array>
#include <cstddef>
#include <cstdint>
#include <map>
#include <unordered_map>
#include <vector>

#include "olgp_semantic.h"

namespace olgp {

struct BatchOp {
  uint16_t op = 0;
  std::vector<uint8_t> payload;

  bool operator==(const BatchOp& other) const {
    return op == other.op && payload == other.payload;
  }
};

// Wire format: count:u16, then op:u16, payload_len:u32, payload for each op.
std::vector<uint8_t> encode_batch(const std::vector<BatchOp>& ops);
bool decode_batch(const uint8_t* data, size_t size, std::vector<BatchOp>& ops);

class TraceSessions {
 public:
  uint64_t begin();
  bool record(uint64_t id, Op op, uint64_t template_hash);
  bool replay(uint64_t id, const std::vector<TraceEntry>& entries);
  bool abort(uint64_t id);
  size_t size() const;

 private:
  std::unordered_map<uint64_t, TraceRecorder> sessions_;
};

class ContentCache {
 public:
  bool query(const std::array<uint8_t, 32>& hash) const;
  void put(const std::array<uint8_t, 32>& hash, uint64_t length);
  size_t size() const;

 private:
  std::map<std::array<uint8_t, 32>, uint64_t> blobs_;
};

}  // namespace olgp

extern "C" int olgp_dispatch_cache_query(const uint8_t* hash32);
extern "C" void olgp_dispatch_cache_put(const uint8_t* hash32, uint64_t length);
extern "C" size_t olgp_dispatch_batch_wire_size(const uint8_t* data, size_t size);

#endif
