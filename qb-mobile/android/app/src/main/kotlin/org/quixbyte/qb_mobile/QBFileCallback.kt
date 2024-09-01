package org.quixbyte.qb_mobile

import android.annotation.TargetApi
import android.os.Build
import android.os.ProxyFileDescriptorCallback
import android.util.Log
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream

@TargetApi(Build.VERSION_CODES.O)
class QBFileCallback(val file: File, val mode: String) : ProxyFileDescriptorCallback() {
    // Constants
    private val TAG = "QBFileCallback"

    var write: FileOutputStream
    var read: FileInputStream
    init {
        write = file.outputStream()
        read = file.inputStream()
    }

    override fun onRelease() {
        read.close()
        write.close()
    }

    override fun onWrite(offset: Long, size: Int, data: ByteArray?): Int {
        Log.i(TAG, "onWrite called")

        write.write(data, offset.toInt(), size)
        TODO("notify dart")
        return size
    }

    override fun onRead(offset: Long, size: Int, data: ByteArray?): Int {
        return read.read(data, offset.toInt(), size)
    }

    override fun onGetSize(): Long {
        return file.length()
    }
}