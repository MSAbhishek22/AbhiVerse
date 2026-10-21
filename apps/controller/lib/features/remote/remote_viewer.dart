import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_webrtc/flutter_webrtc.dart';

import '../../core/api_client.dart';
import '../../core/health_client.dart';
import '../../core/input_events.dart';

/// Remote desktop viewer (Task 006b).
///
/// Flow: create a peer connection + the `abhiverse-input` data
/// channel, offer it through the coordination API (signed, 006a),
/// wait for the person at the computer to approve the session at
/// the agent prompt, apply the signed answer, trickle ICE both
/// ways, render the video track, and send touch input as protocol
/// `InputEvent`s (injected only if the agent runs `--allow-input`).
class RemoteViewerPage extends StatefulWidget {
  const RemoteViewerPage({super.key, required this.device, required this.api});

  final DeviceView device;
  final ApiClient api;

  @override
  State<RemoteViewerPage> createState() => _RemoteViewerPageState();
}

class _RemoteViewerPageState extends State<RemoteViewerPage> {
  final RTCVideoRenderer _renderer = RTCVideoRenderer();
  RTCPeerConnection? _pc;
  RTCDataChannel? _input;
  String? _sessionId;
  String _status = 'Preparing…';
  bool _answerApplied = false;
  bool _videoLive = false;
  bool _closing = false;
  int _iceAfter = 0;
  Timer? _poll;
  DateTime _lastMoveSent = DateTime.fromMillisecondsSinceEpoch(0);
  final TextEditingController _typeController = TextEditingController();

  ApiClient get api => widget.api;

  @override
  void initState() {
    super.initState();
    _start();
  }

  Future<void> _start() async {
    try {
      await _renderer.initialize();
      final iceServers = await api.fetchRtcConfig();
      final pc = await createPeerConnection({
        'iceServers': [
          for (final s in iceServers)
            {
              'urls': s.urls,
              if (s.username != null) 'username': s.username,
              if (s.credential != null) 'credential': s.credential,
            },
        ],
        'sdpSemantics': 'unified-plan',
      });
      _pc = pc;
      _input = await pc.createDataChannel(
        'abhiverse-input',
        RTCDataChannelInit()..ordered = true,
      );
      await pc.addTransceiver(
        kind: RTCRtpMediaType.RTCRtpMediaTypeVideo,
        init: RTCRtpTransceiverInit(
          direction: TransceiverDirection.RecvOnly,
        ),
      );
      pc.onTrack = (event) {
        if (event.streams.isNotEmpty && mounted) {
          setState(() {
            _renderer.srcObject = event.streams[0];
            _videoLive = true;
            _status = 'Connected';
          });
        }
      };
      pc.onIceCandidate = (candidate) {
        final sid = _sessionId;
        if (sid == null || candidate.candidate == null) return;
        api
            .postIceCandidate(
              sessionId: sid,
              candidate: candidate.candidate!,
              sdpMid: candidate.sdpMid,
              sdpMlineIndex: candidate.sdpMLineIndex,
            )
            .catchError((_) {});
      };
      final offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      final session = await api.createSession(
        deviceId: widget.device.deviceId,
        sdpOffer: offer.sdp ?? '',
      );
      _sessionId = session.sessionId;
      if (!mounted) return;
      setState(() => _status =
          'Waiting for approval on ${widget.device.name} — press "y" at the agent prompt…');
      _poll = Timer.periodic(
        const Duration(milliseconds: 800),
        (_) => _pollSession(),
      );
    } on AbhiverseApiException catch (e) {
      if (mounted) setState(() => _status = e.message);
    } catch (e) {
      if (mounted) setState(() => _status = 'Could not start: $e');
    }
  }

  Future<void> _pollSession() async {
    final sid = _sessionId;
    final pc = _pc;
    if (sid == null || pc == null || _closing) return;
    try {
      if (!_answerApplied) {
        final view = await api.fetchSession(sid);
        if (view.status == 'answered' && view.sdpAnswer != null) {
          await pc.setRemoteDescription(
            RTCSessionDescription(view.sdpAnswer!, 'answer'),
          );
          _answerApplied = true;
          if (mounted) {
            setState(() => _status = 'Approved — connecting…');
          }
        } else if (view.status == 'rejected') {
          _finish('Session rejected on the computer.');
          return;
        } else if (view.status == 'ended' || view.status == 'expired') {
          _finish('Session ${view.status}.');
          return;
        }
      }
      final candidates = await api.fetchIceCandidates(
        sessionId: sid,
        after: _iceAfter,
      );
      for (final c in candidates) {
        final seq = (c['seq'] as num?)?.toInt() ?? 0;
        if (seq > _iceAfter) _iceAfter = seq;
        await pc.addCandidate(RTCIceCandidate(
          c['candidate'] as String? ?? '',
          c['sdp_mid'] as String?,
          (c['sdp_mline_index'] as num?)?.toInt(),
        ));
      }
    } catch (_) {
      // Transient polling errors are fine; the timer retries.
    }
  }

  void _sendAll(List<Map<String, dynamic>> events) {
    final channel = _input;
    if (channel == null || channel.state != RTCDataChannelState.RTCDataChannelOpen) {
      return;
    }
    for (final event in events) {
      channel.send(RTCDataChannelMessage(jsonEncode(event)));
    }
  }

  (double, double) _normalised(Offset local, Size size) => (
        (local.dx / size.width).clamp(0.0, 1.0),
        (local.dy / size.height).clamp(0.0, 1.0),
      );

  void _finish(String message) {
    _poll?.cancel();
    if (mounted) setState(() => _status = message);
  }

  Future<void> _endAndPop() async {
    _closing = true;
    _poll?.cancel();
    final sid = _sessionId;
    if (sid != null) {
      try {
        await api.endSession(sid);
      } catch (_) {}
    }
    await _pc?.close();
    if (mounted) Navigator.of(context).pop();
  }

  @override
  void dispose() {
    _closing = true;
    _poll?.cancel();
    _pc?.close();
    _renderer.dispose();
    _typeController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.device.name),
        actions: [
          TextButton.icon(
            onPressed: _endAndPop,
            icon: const Icon(Icons.call_end, color: Colors.redAccent),
            label: const Text('End'),
          ),
        ],
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(8),
            child: Text(_status, textAlign: TextAlign.center),
          ),
          Expanded(
            child: LayoutBuilder(
              builder: (context, constraints) {
                final size = Size(constraints.maxWidth, constraints.maxHeight);
                return GestureDetector(
                  onTapUp: (details) {
                    final (x, y) = _normalised(details.localPosition, size);
                    _sendAll(tapSequence(x, y));
                  },
                  onLongPressStart: (details) {
                    final (x, y) = _normalised(details.localPosition, size);
                    _sendAll(tapSequence(x, y, button: 2));
                  },
                  onPanUpdate: (details) {
                    final now = DateTime.now();
                    if (now.difference(_lastMoveSent).inMilliseconds < 40) {
                      return;
                    }
                    _lastMoveSent = now;
                    final (x, y) = _normalised(details.localPosition, size);
                    _sendAll([mouseMove(x, y)]);
                  },
                  child: Container(
                    color: Colors.black,
                    child: _videoLive
                        ? RTCVideoView(
                            _renderer,
                            objectFit:
                                RTCVideoViewObjectFit.RTCVideoViewObjectFitContain,
                          )
                        : const Center(
                            child: CircularProgressIndicator(),
                          ),
                  ),
                );
              },
            ),
          ),
          _controls(),
        ],
      ),
    );
  }

  Widget _controls() {
    const keys = <(String, String)>[
      ('Esc', 'Escape'),
      ('Tab', 'Tab'),
      ('Enter', 'Enter'),
      ('⌫', 'Backspace'),
      ('←', 'ArrowLeft'),
      ('↑', 'ArrowUp'),
      ('↓', 'ArrowDown'),
      ('→', 'ArrowRight'),
    ];
    return SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(
              children: [
                for (final (label, code) in keys)
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 2),
                    child: OutlinedButton(
                      onPressed: () => _sendAll(keyTap(code)),
                      child: Text(label),
                    ),
                  ),
              ],
            ),
          ),
          Row(
            children: [
              Expanded(
                child: TextField(
                  controller: _typeController,
                  decoration: const InputDecoration(
                    hintText: 'Type text to send…',
                    contentPadding: EdgeInsets.symmetric(horizontal: 12),
                  ),
                  onSubmitted: (value) {
                    if (value.isNotEmpty) {
                      _sendAll([textEvent(value)]);
                      _typeController.clear();
                    }
                  },
                ),
              ),
              IconButton(
                icon: const Icon(Icons.send),
                tooltip: 'Send text',
                onPressed: () {
                  final value = _typeController.text;
                  if (value.isNotEmpty) {
                    _sendAll([textEvent(value)]);
                    _typeController.clear();
                  }
                },
              ),
            ],
          ),
        ],
      ),
    );
  }
}
