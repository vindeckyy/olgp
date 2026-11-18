#include "olgp_dispatch.h"

#include <array>
#include <cassert>
#include <cstdint>
#include <vector>

int main() {
  std::vector<olgp::BatchOp> ops;
  ops.push_back({0x8003, {1, 2, 3}});
  ops.push_back({0x8009, {9}});
  const auto encoded = olgp::encode_batch(ops);
  assert(encoded.size() == 2 + (2 + 4 + 3) + (2 + 4 + 1));

  std::vector<olgp::BatchOp> decoded;
  assert(olgp::decode_batch(encoded.data(), encoded.size(), decoded));
  assert(decoded == ops);

  auto truncated = encoded;
  truncated.pop_back();
  assert(!olgp::decode_batch(truncated.data(), truncated.size(), decoded));
  assert(olgp_dispatch_batch_wire_size(encoded.data(), encoded.size()) == encoded.size());
  assert(!olgp_dispatch_batch_wire_size(nullptr, 0));

  olgp::TraceSessions traces;
  const uint64_t id = traces.begin();
  assert(traces.record(id, olgp::Op::Launch, 42));
  assert(!traces.record(id, olgp::Op::CtxDestroy, 42));
  assert(traces.replay(id, {{olgp::Op::Launch, 42}}));
  assert(!traces.replay(id, {{olgp::Op::Launch, 7}}));
  assert(traces.size() == 0);
  const uint64_t second = traces.begin();
  assert(traces.abort(second));
  assert(!traces.abort(second));

  std::array<uint8_t, 32> hash{};
  hash[0] = 7;
  assert(olgp_dispatch_cache_query(hash.data()) == 0);
  olgp_dispatch_cache_put(hash.data(), 1024);
  assert(olgp_dispatch_cache_query(hash.data()) == 1);
  olgp::ContentCache local;
  assert(!local.query(hash));
  local.put(hash, 2048);
  assert(local.query(hash));
  assert(local.size() == 1);

  return 0;
}
