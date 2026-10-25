import 'dart:convert';
import 'dart:io' show Platform;
import 'dart:math';

import 'package:crypto/crypto.dart' as hash;
import 'package:cryptography/cryptography.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

String _hex(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

/// This phone's persistent device identity (Task 007).
///
/// An Ed25519 key pair generated once and kept in secure storage.
/// The device ID is the fingerprint of the public key — the same
/// scheme as agents — so the API can verify this controller's
/// signatures on transfers, session requests and revocations.
class ControllerIdentity {
  ControllerIdentity._(this._keyPair, this.deviceId, this.publicKeyHex, this.name);

  final SimpleKeyPair _keyPair;
  final String deviceId;
  final String publicKeyHex;
  final String name;

  static const _seedKey = 'abhiverse_controller_seed_v1';

  /// Load the stored identity, or create and persist a new one.
  static Future<ControllerIdentity> loadOrCreate({
    String name = 'Android Controller',
    FlutterSecureStorage? storage,
  }) async {
    final store = storage ?? const FlutterSecureStorage();
    final stored = await store.read(key: _seedKey);
    if (stored != null) {
      return fromSeed(base64Decode(stored), name: name);
    }
    final seed = List<int>.generate(32, (_) => Random.secure().nextInt(256));
    await store.write(key: _seedKey, value: base64Encode(seed));
    return fromSeed(seed, name: name);
  }

  /// Build an identity from a raw 32-byte seed (used by tests too).
  static Future<ControllerIdentity> fromSeed(
    List<int> seed, {
    String name = 'Android Controller',
  }) async {
    final keyPair = await Ed25519().newKeyPairFromSeed(seed);
    final publicKey = await keyPair.extractPublicKey();
    final digest = hash.sha256.convert(publicKey.bytes).bytes;
    return ControllerIdentity._(
      keyPair,
      _hex(digest.sublist(0, 16)),
      _hex(publicKey.bytes),
      name,
    );
  }

  /// Sign a protocol message; returns the hex signature the API
  /// expects in the `x-abhiverse-signature` header.
  Future<String> sign(String message) async {
    final signature = await Ed25519().sign(
      utf8.encode(message),
      keyPair: _keyPair,
    );
    return _hex(signature.bytes);
  }

  /// The `DeviceInfo` JSON the API expects in pairing claims.
  Map<String, dynamic> deviceInfoJson({String? nameOverride}) {
    var platform = 'android';
    try {
      platform = Platform.operatingSystem;
    } catch (_) {
      // Non-IO build (web): keep the default label.
    }
    return {
      'device_id': deviceId,
      'name': nameOverride ?? name,
      'platform': platform,
      'kind': 'controller',
      'public_key': publicKeyHex,
    };
  }
}

/// Canonical protocol signing messages (mirror the Rust protocol crate).
String transferCreateMessage({
  required String fromId,
  required String toId,
  required String sha256Hex,
  required int timestamp,
  required String requestId,
}) =>
    'abhiverse-transfer-create:$fromId:$toId:$sha256Hex:$timestamp:$requestId';

String sessionCreateMessage({
  required String deviceId,
  required String controllerId,
  required String offerSha256Hex,
  required int timestamp,
  required String requestId,
}) =>
    'abhiverse-session-create:$deviceId:$controllerId:$offerSha256Hex:$timestamp:$requestId';

/// A fresh per-request ID (32 hex chars) covered by create-request
/// signatures, so a duplicated create is distinguishable from a
/// replayed one.
String newRequestId() {
  final rng = Random.secure();
  return List<int>.generate(16, (_) => rng.nextInt(256))
      .map((b) => b.toRadixString(16).padLeft(2, '0'))
      .join();
}

String revokeMessage({required String deviceId, required int timestamp}) =>
    'abhiverse-revoke:$deviceId:$timestamp';

/// Signed-GET message (mirror of Rust `signed_get_message`): the
/// caller signs the exact path + query it sends, so a signed read
/// cannot be transplanted onto another resource.
String signedGetMessage({
  required String pathAndQuery,
  required String callerId,
  required int timestamp,
}) =>
    'abhiverse-get:$pathAndQuery:$callerId:$timestamp';

/// ICE trickle message (mirror of Rust `ice_candidate_message`).
String iceCandidateMessage({
  required String sessionId,
  required String fromId,
  required String candidateSha256Hex,
  required int timestamp,
}) =>
    'abhiverse-ice:$sessionId:$fromId:$candidateSha256Hex:$timestamp';

/// Chunk upload/download messages (mirror of Rust).
String chunkUploadMessage({
  required String transferId,
  required int index,
  required String chunkSha256Hex,
  required int timestamp,
}) =>
    'abhiverse-chunk:$transferId:$index:$chunkSha256Hex:$timestamp';

String chunkDownloadMessage({
  required String transferId,
  required int index,
  required int timestamp,
}) =>
    'abhiverse-download:$transferId:$index:$timestamp';

String transferCompleteMessage({
  required String transferId,
  required int timestamp,
}) =>
    'abhiverse-transfer-complete:$transferId:$timestamp';

String sessionEndMessage({
  required String sessionId,
  required int timestamp,
}) =>
    'abhiverse-session-end:$sessionId:$timestamp';

int nowUnix() => DateTime.now().millisecondsSinceEpoch ~/ 1000;
