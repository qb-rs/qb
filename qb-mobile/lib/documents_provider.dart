import 'dart:ui';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_background_service/flutter_background_service.dart';

const CHANNEL_ID = "org.quixbyte.qb_mobile/documents_provider";
const CHANNEL = MethodChannel(CHANNEL_ID);

@pragma('vm:entry-point')
void init() {
  WidgetsFlutterBinding.ensureInitialized();
  DartPluginRegistrant.ensureInitialized();
  var service = FlutterBackgroundService();

  CHANNEL.setMethodCallHandler((call) async {
    switch (call.method) {
      case "notify":
        return service.invoke("notify", {
          "data": call.arguments as String,
        });
    }
  });
}
