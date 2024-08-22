import 'dart:io';
import 'dart:ui';
import 'dart:async';
import 'package:device_info_plus/device_info_plus.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:process/process.dart';

import 'package:flutter/material.dart';
import 'package:flutter_background_service/flutter_background_service.dart';


const ProcessManager processManager = LocalProcessManager();

Future<void> initializeService() async {
  final service = FlutterBackgroundService();

  await service.configure(
    iosConfiguration: IosConfiguration(
      autoStart: true,
      onForeground: onStart,
      onBackground: onIosBackground,
    ),
    androidConfiguration: AndroidConfiguration(
      autoStart: true,
      onStart: onStart,
      isForegroundMode: false,
      autoStartOnBoot: true,
    ),
  );
}

void startBackgroundService() {
  final service = FlutterBackgroundService();
  service.startService();
}

void stopBackgroundService() {
  final service = FlutterBackgroundService();
  service.invoke("stop");
}

@pragma('vm:entry-point')
Future<bool> onIosBackground(ServiceInstance service) async {
  WidgetsFlutterBinding.ensureInitialized();
  DartPluginRegistrant.ensureInitialized();

  return true;
}

@pragma('vm:entry-point')
void onStart(ServiceInstance service) async {
  final dir = await getApplicationSupportDirectory();
  final deviceInfo = DeviceInfoPlugin();
  print("kekw");
  if (Platform.isAndroid) {
    final binaryAbis = ["arm64-v8a", "armeabi-v7a", "x86_64"];
    final androidInfo = await deviceInfo.androidInfo;
    final supportedAbis = androidInfo.supportedAbis;
    final abi = binaryAbis.firstWhere((abi) => supportedAbis.contains(abi));
    //final file = File('${dir.path}/bin/qb-daemon-$abi');
    //if(!await file.exists()) {
    //await copyBinary('qb-daemon-$abi', file);
    //}

    final fileProc = await processManager.run(['ls', '-la', '/data/app/com.example.qb_mobile.apk']);
    print(fileProc.stdout);
    print(fileProc.stderr);

    final file = File('');
    final process = await processManager.run([file.path, '--no-ipc --std'], runInShell: true);
    print(process.stderr);
    process.stdout.listen((data) {
      print("recv: $data");
    });
  } else {
    throw UnimplementedError("QuixByte does not support this device (yet)!");
  }

  service.on("stop").listen((event) {
    service.stopSelf();
    print("background process is now stopped");
  });

  service.on("start").listen((event) {
    print("background service started!");
  });

  Timer.periodic(const Duration(seconds: 1), (timer) {
    print("service is successfully running ${DateTime.now().second}");
  });
}

//Future<void> copyBinary(String srcBin, File dst) async {
//  await dst.create(recursive: true);
//  final src = await rootBundle.load("assets/bin/$srcBin");
//  await dst.writeAsBytes(src.buffer.asUint8List(src.offsetInBytes, src.lengthInBytes));
//  print(src.lengthInBytes);
//  
//  final chmodProc = await processManager.run(['chmod', '+x', dst.path], runInShell: true);
//  print(chmodProc.stdout);
//  print(chmodProc.stderr);
//
//  final fileProc = await processManager.run(['ls', '-la', dst.path]);
//  print(fileProc.stdout);
//}
