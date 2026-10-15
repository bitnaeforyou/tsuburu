//! Asking Android to do something with a file, and the plumbing that takes.
//!
//! Everything a phone does with a file that is not simply reading or writing
//! it goes through another application - the share sheet, the installer - and
//! an application is reached with an intent, which is a Java call. The file
//! travels as a `content://` URI from the `FileProvider` the Android project
//! declares, because a `file://` path handed to another application has been
//! refused since Android 7.
//!
//! The calls are gathered here rather than beside what uses them: the share
//! sheet and the update need the same half-dozen of them, and getting any one
//! wrong is a crash rather than an error.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicPtr, Ordering};

use jni::objects::{JClass, JObject, JValue};
use jni::sys::JavaVM as RawVm;
use jni::{JNIEnv, JavaVM};

/// `Intent.FLAG_GRANT_READ_URI_PERMISSION` - without it the application the
/// file was handed to, which is a different application, cannot open it.
pub const GRANT_READ: i32 = 0x0000_0001;
/// `Intent.FLAG_ACTIVITY_NEW_TASK`, required of an activity started from
/// anything that is not itself an activity.
pub const NEW_TASK: i32 = 0x1000_0000;

/// What a shard or a backup is, as far as another application is concerned.
///
/// Deliberately the vaguest type there is: the receiver is a drive, a chat or
/// a file manager, and none of them care what is inside. Naming the format
/// instead would have the share sheet offer only whatever claimed `.tsd`,
/// which is nothing.
const ANY_FILE: &str = "application/octet-stream";

/// The Java machine this process is already running.
///
/// Handed over once, as the library is loaded, and never offered again:
/// there is no way to ask for it later. `dlsym` does not answer for it
/// either - `JNI_GetCreatedJavaVMs` lives in a library an app is not allowed
/// to reach into.
static MACHINE: AtomicPtr<RawVm> = AtomicPtr::new(std::ptr::null_mut());

/// Called by the library's entry point with what Android handed it.
pub fn note_machine(vm: *mut std::ffi::c_void) {
    MACHINE.store(vm.cast(), Ordering::Release);
}

fn machine() -> Result<JavaVM, String> {
    let raw = MACHINE.load(Ordering::Acquire);
    if raw.is_null() {
        return Err("the Java machine was never handed over".into());
    }
    unsafe { JavaVM::from_raw(raw) }.map_err(|e| e.to_string())
}

/// Runs `body` on a thread Java knows about, and leaves no exception standing.
///
/// A Java call that threw leaves the exception pending, and a thread that
/// detaches with one pending takes the process down - so a failure here would
/// not be reported, it would end the app.
pub fn attached<T>(body: impl FnOnce(&mut JNIEnv) -> Result<T, String>) -> Result<T, String> {
    let vm = machine()?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let done = body(&mut env);
    if env.exception_check().unwrap_or(false) {
        env.exception_describe().ok();
        env.exception_clear().ok();
    }
    done
}

/// This app's own context.
///
/// `ActivityThread` is a framework class, so it can be reached from a thread
/// that was attached rather than started by Java - which is every thread the
/// server runs on.
pub fn application<'a>(env: &mut JNIEnv<'a>) -> Result<JObject<'a>, String> {
    env.call_static_method(
        "android/app/ActivityThread",
        "currentApplication",
        "()Landroid/app/Application;",
        &[],
    )
    .and_then(|value| value.l())
    .map_err(|e| e.to_string())
}

/// Finds a class that came with the app rather than with Android.
///
/// A thread attached from native code is given the system class loader, which
/// knows nothing the app brought with it. The context's own loader does.
pub fn app_class<'a>(
    env: &mut JNIEnv<'a>,
    context: &JObject,
    name: &str,
) -> Result<JClass<'a>, String> {
    let loader = env
        .call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .and_then(|value| value.l())
        .map_err(|e| e.to_string())?;
    let name = text(env, name)?;
    env.call_method(
        &loader,
        "loadClass",
        "(Ljava/lang/String;)Ljava/lang/Class;",
        &[JValue::Object(&name)],
    )
    .and_then(|value| value.l())
    .map(JClass::from)
    .map_err(|e| e.to_string())
}

pub fn package_name(env: &mut JNIEnv, context: &JObject) -> Result<String, String> {
    let name = env
        .call_method(context, "getPackageName", "()Ljava/lang/String;", &[])
        .and_then(|value| value.l())
        .map_err(|e| e.to_string())?;
    env.get_string(&name.into()).map(Into::into).map_err(|e| e.to_string())
}

pub fn text<'a>(env: &mut JNIEnv<'a>, value: &str) -> Result<JObject<'a>, String> {
    env.new_string(value).map(Into::into).map_err(|e| e.to_string())
}

pub fn intent<'a>(env: &mut JNIEnv<'a>, action: &str) -> Result<JObject<'a>, String> {
    let action = text(env, action)?;
    env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])
        .map_err(|e| e.to_string())
}

pub fn uri<'a>(env: &mut JNIEnv<'a>, value: &str) -> Result<JObject<'a>, String> {
    let value = text(env, value)?;
    env.call_static_method(
        "android/net/Uri",
        "parse",
        "(Ljava/lang/String;)Landroid/net/Uri;",
        &[JValue::Object(&value)],
    )
    .and_then(|value| value.l())
    .map_err(|e| e.to_string())
}

/// A `content://` URI the receiving application may read.
///
/// The authority matches `android:authorities` on the provider in the
/// manifest, and the path has to be one `file_paths.xml` covers or this
/// throws rather than returning anything.
pub fn uri_for_file<'a>(
    env: &mut JNIEnv<'a>,
    context: &JObject,
    path: &Path,
) -> Result<JObject<'a>, String> {
    let name = package_name(env, context)?;
    let authority = text(env, &format!("{name}.fileprovider"))?;
    let path = text(env, &path.to_string_lossy())?;
    let file = env
        .new_object("java/io/File", "(Ljava/lang/String;)V", &[JValue::Object(&path)])
        .map_err(|e| e.to_string())?;

    let provider = app_class(env, context, "androidx.core.content.FileProvider")?;
    env.call_static_method(
        &provider,
        "getUriForFile",
        "(Landroid/content/Context;Ljava/lang/String;Ljava/io/File;)Landroid/net/Uri;",
        &[JValue::Object(context), JValue::Object(&authority), JValue::Object(&file)],
    )
    .and_then(|value| value.l())
    .map_err(|e| e.to_string())
}

pub fn add_flags(env: &mut JNIEnv, intent: &JObject, flags: i32) -> Result<(), String> {
    env.call_method(intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(flags)])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn start(env: &mut JNIEnv, context: &JObject, intent: &JObject) -> Result<(), String> {
    env.call_method(
        context,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[JValue::Object(intent)],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Opens the share sheet on the files.
pub fn share(paths: &[PathBuf]) -> Result<(), String> {
    attached(|env| {
        let context = application(env)?;
        let sending = compose(env, &context, paths)?;

        // A chooser rather than the intent on its own: without one Android
        // either picks whatever it last used or shows nothing, and "export"
        // that silently went to one app is the bug this is here to fix.
        let chooser = env
            .call_static_method(
                "android/content/Intent",
                "createChooser",
                "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;",
                &[JValue::Object(&sending), JValue::Object(&JObject::null())],
            )
            .and_then(|value| value.l())
            .map_err(|e| e.to_string())?;
        // On the chooser too: it is the intent being started, and the
        // permission it was not given is one the receiver does not get.
        add_flags(env, &chooser, GRANT_READ | NEW_TASK)?;

        start(env, &context, &chooser)
    })
}

/// The intent that carries the files, which is a different one for one file
/// and for several.
fn compose<'a>(
    env: &mut JNIEnv<'a>,
    context: &JObject,
    paths: &[PathBuf],
) -> Result<JObject<'a>, String> {
    let kind = text(env, ANY_FILE)?;
    let key = text(env, "android.intent.extra.STREAM")?;

    let sending = if let [only] = paths {
        let uri = uri_for_file(env, context, only)?;
        let sending = intent(env, "android.intent.action.SEND")?;
        env.call_method(
            &sending,
            "putExtra",
            "(Ljava/lang/String;Landroid/os/Parcelable;)Landroid/content/Intent;",
            &[JValue::Object(&key), JValue::Object(&uri)],
        )
        .map_err(|e| e.to_string())?;
        sending
    } else {
        let list = env.new_object("java/util/ArrayList", "()V", &[]).map_err(|e| e.to_string())?;
        for path in paths {
            let uri = uri_for_file(env, context, path)?;
            env.call_method(&list, "add", "(Ljava/lang/Object;)Z", &[JValue::Object(&uri)])
                .map_err(|e| e.to_string())?;
        }
        let sending = intent(env, "android.intent.action.SEND_MULTIPLE")?;
        env.call_method(
            &sending,
            "putParcelableArrayListExtra",
            "(Ljava/lang/String;Ljava/util/ArrayList;)Landroid/content/Intent;",
            &[JValue::Object(&key), JValue::Object(&list)],
        )
        .map_err(|e| e.to_string())?;
        sending
    };

    env.call_method(
        &sending,
        "setType",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&kind)],
    )
    .map_err(|e| e.to_string())?;
    add_flags(env, &sending, GRANT_READ)?;
    Ok(sending)
}
