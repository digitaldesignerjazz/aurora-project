# NovaNet v0.1 Wire Protocol & Protocol Refinements Specification

**Version:** 0.1.0-draft  
**Date:** 12 June 2026  
**Status:** Draft for Implementation  
**Authors:** Sven Normen Esslinger / Aurora Project  
**Related:** [NovaNet v0.1 Technical Specification](novanet-v0.1-technical-specification.md)

---

## 1. Protocol Refinements & Design Decisions (v0.1)

After review of the high-level spec, the following concrete decisions are locked for v0.1 implementation:

### 1.1 Coordinate Format
- **Format**: Fixed **32-byte (256-bit)** tree coordinates.
- **Encoding**: The coordinate represents the path from the current spanning tree root. It is constructed as a sequence of 4-byte hop labels (NodeID fragments) or compact prefix. 
  - Byte 0: `coord_len` (number of significant bytes, 4–32)
  - Bytes 1..=coord_len: the coordinate bytes (big-endian path segments)
  - Remaining bytes padded with 0x00 (ignored in comparison)
- **Routing metric**: Longest common prefix match + remaining distance (Hamming or simple byte-wise). Greedy choice minimizes remaining distance to destination coordinate.
- **Why fixed 32B**: Simple array handling in Rust (`[u8; 32]`), efficient comparison, room for future embedding of latency/XCoin scores in lower bits without resizing. Matches common cryptographic sizes.
- **Tree root election**: Node with the **lexicographically smallest NodeID** (Ed25519 pubkey bytes) in the connected component becomes root. Stable and simple; rotation logic deferred to v0.2.

### 1.2 Bloom Filter
- **Size**: **2048 bits (256 bytes)** 
- **Hash functions (k)**: 4 (using BLAKE3 truncated + double hashing technique for speed)
- **Purpose in v0.1**: Summarize the set of reachable NodeIDs (and their tree subtrees) advertised by a peer. Used to quickly cull impossible forwarding destinations before expensive coordinate math.
- **False positive rate target**: < 1% for ~400–600 entries (typical small-mesh view).
- **Update frequency**: On every `TreeAnnounce` (seq number prevents replay). Nodes merge received bloom filters into their own view.
- **Implementation**: `fastbloom` crate or custom `BitVec` + BLAKE3. Serialized as raw 256 bytes in `TreeAnnounce`.

### 1.3 Yggdrasil Wire Compatibility
**Decision for v0.1: NO full wire compatibility.**

- NovaNet uses its own message types, Noise prologue (`"novanet/0.1" + chain_id`), and framing.
- **Rationale**:
  - Allows immediate integration of Nexus message types (0x08) and future SolNet/XCoin fields without legacy constraints.
  - Cleaner security defaults and audit surface.
  - Avoids carrying Yggdrasil's specific coordinate encoding quirks or older crypto choices.
- **v0.2+ Path**: A compatibility layer / dual-stack peering module will be added. A NovaNet node will be able to speak both protocols on different sockets or via a translator daemon. This enables gradual onboarding of existing Yggdrasil meshes while we bootstrap the Aurora-native network.
- **Interop note**: NodeIDs remain Ed25519-based (compatible in principle). A future "YggBridge" in SolNet could translate coordinates.

These decisions keep v0.1 **focused, auditable, and Aurora-native** while leaving clear evolution paths.

---

## 2. Connection Lifecycle

1. **Transport**: TCP (primary for v0.1), optional UDP/QUIC later.
2. **Handshake**: Noise Protocol Framework  
   - Pattern: `Noise_XX_25519_ChaChaPoly_BLAKE2s` (or BLAKE3 variant via custom)
   - Prologue: `b"novanet/0.1"` (4 bytes magic + version)
   - After successful handshake: mutual Ed25519 authentication, 2x 32-byte session keys (send/recv), forward secrecy.
3. **Post-handshake**: All further messages framed and encrypted under the Noise transport.

---

## 3. Message Framing (on encrypted session)

```
Offset  Size   Field
0       4      length (u32 BE) — total bytes following this field (type + payload)
4       1      type (u8)
5       ...    payload (length bytes)
```

- Maximum message size (v0.1): 64 KiB (enforced; larger Data messages fragment at session layer).
- Heartbeat: Nodes send `Ping` every 15–30s idle. Timeout after 3 missed → consider peer unhealthy.

---

## 4. Message Types & Exact Binary Layout

### 4.1 Ping (0x01)
```
u64   nonce          (random, for matching)
u64   timestamp_ms   (sender's monotonic clock)
```
Purpose: Keepalive + one-way latency measurement.

### 4.2 Pong (0x02)
```
u64   nonce
u64   timestamp_ms   (echo of Ping)
u16   latency_ms     (measured round-trip by Pong sender)
```

### 4.3 TreeAnnounce (0x03) — Core routing gossip
```
u8    coord_len      (4–32)
[u8; coord_len] tree_coordinate
u16   bloom_len      (must be 256 for v0.1)
[u8; 256] bloom_filter
u64   seq            (monotonic per sender, starts at 0)
u8    flags          (bit 0 = is_bridge, bit 1 = prefers_high_quality, ...)
```
- Sent periodically (every 30–60s) and on significant topology change.
- Receiver merges bloom and updates coordinate view for that peer.

### 4.4 RouteUpdate (0x04) — Shortcut / path improvement (stub for v0.1)
```
u8    update_type    (0 = offer_shortcut, 1 = accept, 2 = withdraw)
[32]  target_node_id
[u8; 32] suggested_coordinate   (or full path info in future)
u32   metric         (lower = better; composite of latency + quality)
```
v0.1 implementation may ignore or minimally process for basic greedy routing.

### 4.5 Data (0x05) — Encapsulated user / upper-layer payload
```
u64   session_id     (unique per end-to-end flow, chosen by source)
[32]  dst_node_id
u32   payload_len
[u8; payload_len] payload
u8    flags          (bit 0 = e2e_encrypted, bit 1 = priority, ...)
```
- Intermediate nodes **do not** decrypt payload (only session header for forwarding decision).
- End-to-end encryption (if enabled) is handled by SolNet/Nexus or application layer.

### 4.6 PathSetup (0x06) — Request optimized path (v0.1 minimal)
```
u8    setup_type     (0 = request, 1 = offer, 2 = confirm)
[32]  dst_node_id
[u8; 32] via_coordinate
u64   path_id
```
Used for explicit source-routed or shortcut paths. v0.1 may fall back to pure greedy + tree.

### 4.7 BridgeAnnounce (0x07)
```
u8    bridge_types   (bitmask: 1=Tor, 2=I2P, 4=clearnet_egress)
[32]  bridge_addr_hash   (hash of .onion or .b32.i2p or IP)
u16   port
u8    policy_flags
```
Announces bridge capability. Peers can then initiate peering over the announced transport.

### 4.8 NexusMessage (0x08) — Stub for Aurora integration
```
u16   msg_type       (Nexus-defined)
u32   payload_len
[u8; payload_len] payload
```
Opaque to NovaNet core. Passed to local Nexus agent via gRPC or internal channel. Enables agent swarm control traffic to ride the mesh.

---

## 5. NodeID & Addressing

- **NodeID**: 32 bytes = Ed25519 public key (raw, uncompressed)
- **ULA IPv6**: `fd00:dead:beef::/48` + (NodeID as last 80 bits, or BLAKE3(NodeID) truncated). Configurable prefix.
- Applications use normal IPv6 sockets on the `nova0` TUN interface.

---

## 6. Error & Control (future extension point)

Reserved type `0xFF` for `Error` with u8 code + optional message.

Current v0.1 nodes should ignore unknown types gracefully (for forward compatibility).

---

## 7. Security Notes Specific to Wire

- All post-handshake traffic is encrypted and authenticated via Noise.
- `TreeAnnounce` seq prevents replay of old coordinates.
- Bloom filters are not authenticated beyond link encryption (acceptable for v0.1; future signed announcements).
- Session layer provides confidentiality for user Data payloads between source and destination (link encryption protects metadata on each hop).

---

**This document + the main Technical Specification together form the complete v0.1 protocol definition.**

Implementation of `novanetd` must follow these exact layouts for interoperability.

---

*End of NovaNet v0.1 Wire Protocol Specification*  
Aurora Project – 12 June 2026