#include "olgp_dispatch.h"

#include <limits>

namespace olgp {
namespace {

void put_u16(std::vector<uint8_t>& out, uint16_t value) {
  out.push_back(static_cast<uint8_t>(value));
  out.push_back(static_cast<uint8_t>(value >> 8));
}

void put_u32(std::vector<uint8_t>& out, uint32_t value) {
  for (int shift = 0; shift < 32; shift += 8) {
    out.push_back(static_cast<uint8_t>(value >> shift));
  }
}
olgp::ContentCache& global_cache() {
  static olgp::ContentCache cache;
  return cache;
}

bool read_u16(const uint8_t* data, size_t size, size_t& at, uint16_t& value) {
  if (at + 2 > size) return false;
  value = static_cast<uint16_t>(data[at]) |
          static_cast<uint16_t>(static_cast<uint16_t>(data[at + 1]) << 8);
  at += 2;
  return true;
}

bool read_u32(const uint8_t* data, size_t size, size_t& at, uint32_t& value) {
  if (at + 4 > size) return false;
  value = 0;
  for (int shift = 0; shift < 32; shift += 8) {
    value |= static_cast<uint32_t>(data[at + shift / 8]) << shift;
  }
  at += 4;
  return true;
}

}  // namespace

std::vector<uint8_t> encode_batch(const std::vector<BatchOp>& ops) {
  if (ops.size() > std::numeric_limits<uint16_t>::max()) return {};
  std::vector<uint8_t> out;
  put_u16(out, static_cast<uint16_t>(ops.size()));
  for (const auto& op : ops) {
    if (op.payload.size() > std::numeric_limits<uint32_t>::max()) return {};
    put_u16(out, op.op);
    put_u32(out, static_cast<uint32_t>(op.payload.size()));
    out.insert(out.end(), op.payload.begin(), op.payload.end());
  }
  return out;
}

bool decode_batch(const uint8_t* data, size_t size, std::vector<BatchOp>& ops) {
  if (data == nullptr) return false;
  size_t at = 0;
  uint16_t count = 0;
  if (!read_u16(data, size, at, count)) return false;
  ops.clear();
  ops.reserve(count);
  for (uint16_t index = 0; index < count; ++index) {
    BatchOp op;
    uint32_t length = 0;
    if (!read_u16(data, size, at, op.op)) return false;
    if (!read_u32(data, size, at, length)) return false;
    if (length > size - at) return false;
    op.payload.assign(data + at, data + at + length);
    at += length;
    ops.push_back(std::move(op));
  }
  return at == size;
}

uint64_t TraceSessions::begin() {
  static uint64_t next_id = 0;
  const uint64_t id = ++next_id;
  sessions_.emplace(id, TraceRecorder{});
  return id;
}

bool TraceSessions::record(uint64_t id, Op op, uint64_t template_hash) {
  auto it = sessions_.find(id);
  if (it == sessions_.end() || !TraceRecorder::safe(op)) return false;
  it->second.record(op, template_hash);
  return true;
}

bool TraceSessions::replay(uint64_t id, const std::vector<TraceEntry>& entries) {
  auto it = sessions_.find(id);
  if (it == sessions_.end()) return false;
  const bool matches = it->second.validate(entries);
  if (!matches) {
    ++counters().trace_aborts;
    sessions_.erase(it);
  }
  return matches;
}

bool TraceSessions::abort(uint64_t id) {
  const bool removed = sessions_.erase(id) != 0;
  if (removed) ++counters().trace_aborts;
  return removed;
}

size_t TraceSessions::size() const { return sessions_.size(); }

bool ContentCache::query(const std::array<uint8_t, 32>& hash) const {
  const bool hit = blobs_.find(hash) != blobs_.end();
  if (hit) {
    ++counters().cache_hits;
  } else {
    ++counters().cache_misses;
  }
  return hit;
}

void ContentCache::put(const std::array<uint8_t, 32>& hash, uint64_t length) {
  blobs_[hash] = length;
}

size_t ContentCache::size() const { return blobs_.size(); }

}  // namespace olgp

extern "C" int olgp_dispatch_cache_query(const uint8_t* hash32) {
  if (hash32 == nullptr) return 0;
  std::array<uint8_t, 32> hash{};
  for (size_t i = 0; i < hash.size(); ++i) hash[i] = hash32[i];
  olgp::ContentCache& cache = olgp::global_cache();
  return cache.query(hash) ? 1 : 0;
}

extern "C" void olgp_dispatch_cache_put(const uint8_t* hash32, uint64_t length) {
  if (hash32 == nullptr) return;
  std::array<uint8_t, 32> hash{};
  for (size_t i = 0; i < hash.size(); ++i) hash[i] = hash32[i];
  olgp::ContentCache& cache = olgp::global_cache();
  cache.put(hash, length);
}

extern "C" size_t olgp_dispatch_batch_wire_size(const uint8_t* data, size_t size) {
  std::vector<olgp::BatchOp> ops;
  if (!olgp::decode_batch(data, size, ops)) return 0;
  return olgp::encode_batch(ops).size();
}
