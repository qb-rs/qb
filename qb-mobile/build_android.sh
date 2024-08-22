# This shell file is for building android specific
# files required for the application to work.

cd ..
# Build the daemon for android
cargo ndk \
  -t armeabi-v7a \
  -t arm64-v8a \
  -t x86_64 \
  build \
  --bin qb-daemon \
  --release \
  --no-default-features \
  --features ring

mkdir -p qb-mobile/bin
cp target/armv7-linux-androideabi/release/qb-daemon qb-mobile/assets/bin/qb-daemon-armeabi-v7a
cp target/aarch64-linux-android/release/qb-daemon qb-mobile/assets/bin/qb-daemon-arm64-v8a
cp target/x86_64-linux-android/release/qb-daemon qb-mobile/assets/bin/qb-daemon-x86_64
