package org.quixbyte.qb_mobile

import android.net.Uri;
import android.os.AsyncTask;
import android.os.AsyncTask.Status;
import android.util.Log;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.Executor;
import java.util.concurrent.Executors;

class QBTaskManager {
    private val TAG = "TaskManager";

    private val executor = Executors.newCachedThreadPool();

    public fun runTask(task: () -> Unit) {
        executor.execute(task)
    }
}