package org.quixbyte.qb_mobile

import android.annotation.TargetApi
import android.os.Build
import android.os.ProxyFileDescriptorCallback
import android.util.Log
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import java.nio.file.attribute.BasicFileAttributes

@TargetApi(Build.VERSION_CODES.O)
class QBFileCallback(
    private val file: File,
    private val mode: String,
    private val onWriteCB: () -> Unit,
) :
    ProxyFileDescriptorCallback() {
    // Constants
    private val TAG = "QBFileCallback"

    override fun onWrite(offset: Long, size: Int, data: ByteArray?): Int {
        Log.i(TAG, "onWrite called")

        var fileOs = file.outputStream()
        fileOs.write(data, offset.toInt(), size)
        fileOs.flush()
        fileOs.close()

        onWriteCB()

        return size
    }

    override fun onRelease() {}

    override fun onRead(offset: Long, size: Int, data: ByteArray?): Int {
        var fileIs = file.inputStream()
        var size = fileIs.read(data, offset.toInt(), size)
        fileIs.close()

        // size can be -1 if the file size is 0. This causes the app to crash.
        return Math.max(size, 0)
    }

    override fun onGetSize(): Long {
        // this should yield better exceptions compared to .length()
        var attr = Files.readAttributes(Paths.get(file.path), BasicFileAttributes::class.java)
        return attr.size()
    }
}