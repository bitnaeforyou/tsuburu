//! Handing a downloaded APK to the system installer.
//!
//! A phone has no equivalent of writing over the file that is running: an app
//! is replaced by the package installer, which only the operating system may
//! run and which only starts for an intent. So the update ends here rather
//! than in `swap` - the bytes are fetched the same way, and then the installer
//! takes over and asks the reader to confirm.
//!
//! The file is handed over as a `content://` URI from the `FileProvider` the
//! Android project declares, because a `file://` path to another application
//! has been refused since Android 7.

use std::path::Path;

use jni::objects::{JClass, JObject, JValue};
use jni::sys::{JavaVM as RawVm, jint, jsize};
use jni::{JNIEnv, JavaVM};

/// `Intent.FLAG_GRANT_READ_URI_PERMISSION` - without it the installer, which
/// is a different application, cannot open the file it was handed.
const GRANT_READ: i32 = 0x0000_0001;
/// `Intent.FLAG_ACTIVITY_NEW_TASK`, required of an activity started from
/// anything that is not itself an activity.
const NEW_TASK: i32 = 0x1000_0000;

const PACKAGE: &str = "application/vnd.android.package-archive";

/// Opens the installer on `apk`.
///
/// Returns once the intent has been handed over; whether the reader goes
/// through with it is between them and the operating system.
pub fn install(apk: &Path) -> Result<(), String> {
    let vm = machine()?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let context = application(&mut env)?;

    // The reader has to have allowed this app to install applications, and
    // the only thing that can ask them is the settings screen for it.
    if !may_install(&mut env, &context)? {
        return ask_to_allow(&mut env, &context);
    }

    // Matches `android:authorities` on the provider in the manifest.
    let name = package_name(&mut env, &context)?;
    let authority = text(&mut env, &format!("{name}.fileprovider"))?;
    let path = text(&mut env, &apk.to_string_lossy())?;
    let file = env
        .new_object("java/io/File", "(Ljava/lang/String;)V", &[JValue::Object(&path)])
        .map_err(|e| e.to_string())?;

    let provider = app_class(&mut env, &context, "androidx.core.content.FileProvider")?;
    let uri = env
        .call_static_method(
            &provider,
            "getUriForFile",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/io/File;)Landroid/net/Uri;",
            &[JValue::Object(&context), JValue::Object(&authority), JValue::Object(&file)],
        )
        .and_then(|value| value.l())
        .map_err(|e| e.to_string())?;

    let intent = intent(&mut env, "android.intent.action.VIEW")?;
    let kind = text(&mut env, PACKAGE)?;
    env.call_method(
        &intent,
        "setDataAndType",
        "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&uri), JValue::Object(&kind)],
    )
    .map_err(|e| e.to_string())?;
    env.call_method(
        &intent,
        "addFlags",
        "(I)Landroid/content/Intent;",
        &[JValue::Int(GRANT_READ | NEW_TASK)],
    )
    .map_err(|e| e.to_string())?;

    start(&mut env, &context, &intent)
}

/// The Java machine this process is already running.
///
/// Asked for at runtime rather than kept from an entry point, because the
/// entry points here belong to Tauri and none of them hands one over. The
/// symbol is in the runtime that is already loaded, so it is looked up in the
/// process rather than linked against - the NDK publishes no library to link
/// it from.
fn machine() -> Result<JavaVM, String> {
    type Created = unsafe extern "C" fn(*mut *mut RawVm, jsize, *mut jsize) -> jint;

    let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"JNI_GetCreatedJavaVMs".as_ptr()) };
    if symbol.is_null() {
        return Err("this process has no Java machine in it".into());
    }
    let created: Created = unsafe { std::mem::transmute(symbol) };

    let mut raw: *mut RawVm = std::ptr::null_mut();
    let mut found: jsize = 0;
    if unsafe { created(&mut raw, 1, &mut found) } != 0 || found == 0 || raw.is_null() {
        return Err("the Java machine could not be reached".into());
    }
    unsafe { JavaVM::from_raw(raw) }.map_err(|e| e.to_string())
}

/// This app's own context.
///
/// `ActivityThread` is a framework class, so it can be reached from a thread
/// that was attached rather than started by Java - which is every thread the
/// server runs on.
fn application<'a>(env: &mut JNIEnv<'a>) -> Result<JObject<'a>, String> {
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
fn app_class<'a>(
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

fn may_install(env: &mut JNIEnv, context: &JObject) -> Result<bool, String> {
    let packages = env
        .call_method(context, "getPackageManager", "()Landroid/content/pm/PackageManager;", &[])
        .and_then(|value| value.l())
        .map_err(|e| e.to_string())?;
    // Only Android 8 and later asks; below it the permission alone is enough
    // and the method is not there to be called.
    match env.call_method(&packages, "canRequestPackageInstalls", "()Z", &[]) {
        Ok(value) => value.z().map_err(|e| e.to_string()),
        Err(_) => {
            env.exception_clear().ok();
            Ok(true)
        }
    }
}

/// Opens the one settings screen that can grant it, since an app cannot ask
/// for this permission in a dialogue the way it asks for the camera.
fn ask_to_allow(env: &mut JNIEnv, context: &JObject) -> Result<(), String> {
    let intent = intent(env, "android.settings.MANAGE_UNKNOWN_APP_SOURCES")?;
    let name = package_name(env, context)?;
    let uri = uri(env, &format!("package:{name}"))?;
    env.call_method(
        &intent,
        "setData",
        "(Landroid/net/Uri;)Landroid/content/Intent;",
        &[JValue::Object(&uri)],
    )
    .map_err(|e| e.to_string())?;
    env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(NEW_TASK)])
        .map_err(|e| e.to_string())?;
    start(env, context, &intent)?;
    Err("allow tsuburu to install applications on the screen that just opened, then ask again"
        .into())
}

fn package_name(env: &mut JNIEnv, context: &JObject) -> Result<String, String> {
    let name = env
        .call_method(context, "getPackageName", "()Ljava/lang/String;", &[])
        .and_then(|value| value.l())
        .map_err(|e| e.to_string())?;
    env.get_string(&name.into()).map(Into::into).map_err(|e| e.to_string())
}

fn text<'a>(env: &mut JNIEnv<'a>, value: &str) -> Result<JObject<'a>, String> {
    env.new_string(value).map(Into::into).map_err(|e| e.to_string())
}

fn intent<'a>(env: &mut JNIEnv<'a>, action: &str) -> Result<JObject<'a>, String> {
    let action = text(env, action)?;
    env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])
        .map_err(|e| e.to_string())
}

fn uri<'a>(env: &mut JNIEnv<'a>, value: &str) -> Result<JObject<'a>, String> {
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

fn start(env: &mut JNIEnv, context: &JObject, intent: &JObject) -> Result<(), String> {
    env.call_method(
        context,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[JValue::Object(intent)],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
