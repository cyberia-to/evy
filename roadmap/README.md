# evy roadmap

open proposals not yet executed. each is a draft → accepted → migrated-to-specs lifecycle.

## remaining proposals

| proposal | status | what's missing |
|----------|--------|----------------|
| (none yet — first crates land via `bevy/roadmap/cyb-engine-shardstore` in bbg, which blocks evy_ecs_storage) | — | — |

## executed

| former proposal | reference | explanation |
|---|---|---|
| (none yet — evy is pre-implementation) | — | — |

## upstream proposal index

evy depends on proposals tracked in other repos:

- [[bbg/roadmap/cyb-engine-shardstore]] — 5 additions to `ShardStore` (EPHEMERAL dimension, get_mut/mark_dirty/remove, iter, UnimemStore::reserve_pool). blocks `evy_ecs_storage` (step 1 of the spec attack order).
