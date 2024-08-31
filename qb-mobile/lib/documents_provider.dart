import 'dart:ui';
import 'package:flutter/material.dart';
import 'package:flutter_background_service/flutter_background_service.dart';

@pragma('vm:entry-point')
void init() {
  WidgetsFlutterBinding.ensureInitialized();
  DartPluginRegistrant.ensureInitialized();

  Future.delayed(Duration(seconds: 5)).then((_) {
    FlutterBackgroundService().invoke("kekw");
  });
}
