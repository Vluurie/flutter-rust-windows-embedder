import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

void main(List<String> args) {
  runWidget(MultiViewTestApp(maximizeSecondary: args.contains('--maximize-secondary')));
}

/// Channel the embedder exposes for satellite-window controls. Matches the real
/// overlay app's SatelliteWindow channel.
const _windowControlChannel = BasicMessageChannel<String?>(
  'flutter_embedder/satellite_window',
  StringCodec(),
);

void _sendWindowControl(int viewId, String method) {
  _windowControlChannel.send(jsonEncode({'viewId': viewId, 'method': method}));
}

class MultiViewTestApp extends StatefulWidget {
  const MultiViewTestApp({super.key, this.maximizeSecondary = false});

  /// When true, the app sends a `maximize` control message (over the same
  /// channel the real Dart UI uses) for the first secondary view as soon as one
  /// appears. Drives the in-game maximize path from Dart for the maximize test.
  final bool maximizeSecondary;

  @override
  State<MultiViewTestApp> createState() => _MultiViewTestAppState();
}

class _MultiViewTestAppState extends State<MultiViewTestApp>
    with WidgetsBindingObserver {
  final Set<int> _maximized = <int>{};

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangeMetrics() {
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final views = WidgetsBinding.instance.platformDispatcher.views;
    debugPrint(
      'DART_BUILD views=${views.map((v) => '${v.viewId}:${v.physicalSize.width.toInt()}x${v.physicalSize.height.toInt()}').join(',')}',
    );

    if (widget.maximizeSecondary) {
      for (final view in views) {
        if (view.viewId != 0 && !_maximized.contains(view.viewId)) {
          _maximized.add(view.viewId);
          // Send after this frame so the view is fully registered first.
          WidgetsBinding.instance.addPostFrameCallback((_) {
            _sendWindowControl(view.viewId, 'maximize');
          });
        }
      }
    }

    return ViewCollection(
      views: [
        for (final view in views)
          View(
            view: view,
            child: _ViewContent(size: view.physicalSize),
          ),
      ],
    );
  }
}

class _ViewContent extends StatelessWidget {
  const _ViewContent({required this.size});

  final Size size;

  @override
  Widget build(BuildContext context) {
    return Directionality(
      textDirection: TextDirection.ltr,
      child: Container(
        color: const Color(0xFF2244AA),
        alignment: Alignment.center,
        child: Text(
          '${size.width.toInt()}x${size.height.toInt()}',
          style: const TextStyle(color: Color(0xFFFFFFFF), fontSize: 32),
        ),
      ),
    );
  }
}
