# Fake RunPod answers

Hand-written, in the shapes these sources document (read 2026-11-19). No answer here came from
a RunPod account, and the tests never call RunPod.

| File | Command (runpodctl 3.0.1) | Shape from |
|---|---|---|
| `gpu_list.json` | `gpu list --include-unavailable` | runpodctl docs, "GPU pricing and per-data-center availability" (`gpuId`, `securePricePerHr`, `communityPricePerHr` — explicitly `null` when that cloud does not offer the GPU — `stockStatus`, `available`, `dataCenterAvailability[]`), in the RunPod plugin's `skills/runpodctl/reference/output-and-errors.md`. The prices come from GraphQL `gpuTypes { securePrice communityPrice }`. Community prices are the plan's M3 readings; the others are made up for the tests. |
| `pod.json` | `pod get <id>`, `pod create …` | REST v2 "Get a pod" (https://docs.runpod.io/api-reference-v2/pods/get-a-pod.md: `id`, `name`, `status`, `cost`, `gpu`, `startedAt`) and runpodctl's "Pod status fields" (`desiredStatus`, `runtimeStatus`) in `skills/runpodctl/reference/command-reference.md`. |
| `billing.json` | `billing pods --pod-id <id> --bucket-size hour --start-time <t>` | REST v2 "Get pod billing history" (https://docs.runpod.io/api-reference-v2/billing/get-pod-billing-history.md): `{records: [{startTime, endTime, podId, totalAmount, gpuAmount, cpuAmount, diskAmount}], metadata: {query, recordCount, uniquePodCount, totals}}`. |
| `user.json` | `user` | runpodctl reference "user" (https://docs.runpod.io/runpodctl/reference/runpodctl-user.md): `clientBalance`, `currentSpendPerHr`, `spendLimit`, … |
