//! Handing a downloaded APK to the system installer.
//!
//! A phone has no equivalent of writing over the file that is running: an
//! app is replaced by the package installer, which only the operating system
//! may run and which only starts for an intent. So the update ends here
//! rather than in `swap` - the bytes are fetched the same way, and then the
//! installer takes over and asks the reader to confirm.
//!
//! The file is handed over as a `content://` URI from the `FileProvider` the
//! Android project already declares: a `file://` path to another application
//! has been refused since Android 7.

use std::path::Path;

use jni::objects::{JObject, JValue};
use jni::JavaVM;

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
    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(context.context().cast()) };

    // The reader has to have allowed this app to install applications, and
    // the only thing that can ask them is the settings screen for it.
    if !may_install(&mut env, &activity)? {
        return ask_to_allow(&mut env, &activity);
    }

    let name: String = env
        .call_method(&activity, "getPackageName", "()Ljava/lang/String;", &[])
        .and_then(|value| value.l())
        .map_err(|e| e.to_string())
        .and_then(|value| {
            env.get_string(&value.into()).map(|s| s.into()).map_err(|e| e.to_string())
        })?;
    // Matches `android:authorities` on the provider in the manifest.
    let authority = env.new_string(format!("{name}.fileprovider")).map_err(|e| e.to_string())?;

    let path = env.new_string(apk.to_string_lossy().as_ref()).map_err(|e| e.to_string())?;
    let file = env
        .new_object("java/io/File", "(Ljava/lang/String;)V", &[JValue::Object(&path.into())])
        .map_err(|e| e.to_string())?;

    let uri = env
        .call_static_method(
            "androidx/core/content/FileProvider",
            "getUriForFile",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/io/File;)Landroid/net/Uri;",
            &[
                JValue::Object(&activity),
                JValue::Object(&authority.into()),
                JValue::Object(&file),
            ],
        )
        .and_then(|value| value.l())
        .map_err(|e| e.to_string())?;

    let intent = new_intent(&mut env, "android.intent.action.VIEW")?;
    let kind = env.new_string(PACKAGE).map_err(|e| e.to_string())?;
    env.call_method(
        &intent,
        "setDataAndType",
        "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&uri), JValue::Object(&kind.into())],
    )
    .map_err(|e| e.to_string())?;
    env.call_method(
        &intent,
        "addFlags",
        "(I)Landroid/content/Intent;",
        &[JValue::Int(GRANT_READ | NEW_TASK)],
    )
    .map_err(|e| e.to_string())?;

    start(&mut env, &activity, &intent)
}

fn may_install(env: &mut jni::JNIEnv, activity: &JObject) -> Result<bool, String> {
    let packages = env
        .call_method(activity, "getPackageManager", "()Landroid/content/pm/PackageManager;", &[])
        .and_then(|value| value.l())
        .map_err(|e| e.to_string())?;
    // Only Android 8 and later asks; below it the permission alone is enough,
    // and the method does not exist to be called.
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
fn ask_to_allow(env: &mut jni::JNIEnv, activity: &JObject) -> Result<(), String> {
    let intent = new_intent(env, "android.settings.MANAGE_UNKNOWN_APP_SOURCES")?;
    let name: String = env
        .call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])
        .and_then(|value| value.l())
        .map_err(|e| e.to_string())
        .and_then(|value| env.get_string(&value.into()).map(|s| s.into()).map_err(|e| e.to_string()))?;
    let uri = parse_uri(env, &format!("package:{name}"))?;
    env.call_method(
        &intent,
        "setData",
        "(Landroid/net/Uri;)Landroid/content/Intent;",
        &[JValue::Object(&uri)],
    )
    .map_err(|e| e.to_string())?;
    env.call_method(&intent, "addFlags", "(I)Landroid/content/Intent;", &[JValue::Int(NEW_TASK)])
        .map_err(|e| e.to_string())?;
    start(env, activity, &intent)?;
    Err("allow tsuburu to install applications on the screen that just opened, then ask again"
        .into())
}

fn new_intent<'a>(env: &mut jni::JNIEnv<'a>, action: &str) -> Result<JObject<'a>, String> {
    let action = env.new_string(action).map_err(|e| e.to_string())?;
    env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action.into())])
        .map_err(|e| e.to_string())
}

fn parse_uri<'a>(env: &mut jni::JNIEnv<'a>, text: &str) -> Result<JObject<'a>, String> {
    let text = env.new_string(text).map_err(|e| e.to_string())?;
    env.call_static_method(
        "android/net/Uri",
        "parse",
        "(Ljava/lang/String;)Landroid/net/Uri;",
        &[JValue::Object(&text.into())],
    )
    .and_then(|value| value.l())
    .map_err(|e| e.to_string())
}

fn start(env: &mut jni::JNIEnv, activity: &JObject, intent: &JObject) -> Result<(), String> {
    env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(intent)])
        .map_err(|e| e.to_string())?;
    Ok(())
}
