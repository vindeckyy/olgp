#include "olgp_semantic.h"

#include <cassert>
#include <vector>
#include "codegen/gen_rpc_ids.h"
#include "rpc.h"

int main() {
  olgp::BatchRecognizer recognizer;
  auto first = recognizer.note(olgp::Op::MemcpyHtoDAsync);
  assert(first.hold);
  auto second = recognizer.note(olgp::Op::StreamSync);
  assert(!second.hold);
  assert(second.batched.size() == 2);
  assert(recognizer.hits() == 1);

  auto miss_first = recognizer.note(olgp::Op::Launch);
  assert(miss_first.hold);
  auto miss_second = recognizer.note(olgp::Op::EventQuery);
  assert(!miss_second.hold);
  assert(miss_second.batched.empty());
  assert(recognizer.misses() == 1);

  olgp::TraceRecorder recorder;
  recorder.begin();
  recorder.record(olgp::Op::Launch, 0x1234);
  recorder.record(olgp::Op::ModuleLoadData, 0x5678);
  const auto trace_id = recorder.finish();
  assert((trace_id != std::array<uint8_t, 16>{}));
  assert(recorder.size() == 2);
  assert(!recorder.validate({{olgp::Op::Launch, 0x1234}}));

  olgp::HandleTable handles;
  std::array<uint8_t, 32> hash{};
  hash[0] = 7;
  assert(handles.retain(1, olgp::HandleClass::KvCache, false, hash));
  assert(!handles.may_share(1));
  assert(handles.retain(2, olgp::HandleClass::ImmutableWeight, true, hash));
  assert(handles.may_share(2));
  assert(!handles.retain(3, olgp::HandleClass::ImmutableWeight, false, hash));
  assert(handles.release(2));
  assert(!handles.may_share(2));

  assert(olgp::map_lupine_op(RPC_cuLaunchKernel) == olgp::Op::Launch);
  assert(olgp_independent_rpc(RPC_cuLaunchKernel));
  assert(!olgp_independent_rpc(RPC_cuStreamSynchronize));
  assert(olgp::olgp_batch_ops() == 0);
  return 0;
}
