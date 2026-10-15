// Presenting the share sheet on files tsuburu has written.
//
// The share sheet is UIKit, which Rust cannot reach, so the direction is
// reversed: this is called from Rust and does the UIKit part. The counterpart
// on Android is crates/tsuburu-host/src/android.rs, which can make the call
// itself because an intent is a Java call and Rust has a way into Java.

#import <UIKit/UIKit.h>

extern "C" int tsuburu_share_files(const char *const *paths, int count) {
	if (count <= 0 || paths == NULL) {
		return -1;
	}

	NSMutableArray *items = [NSMutableArray arrayWithCapacity:count];
	for (int i = 0; i < count; i++) {
		if (paths[i] == NULL) {
			continue;
		}
		NSString *path = [NSString stringWithUTF8String:paths[i]];
		if (path != nil) {
			[items addObject:[NSURL fileURLWithPath:path]];
		}
	}
	if (items.count == 0) {
		return -1;
	}

	// UIKit may only be touched on the main thread, and this is called from
	// whichever thread the server answered the request on. Asynchronously
	// because the sheet stays up until the reader is done with it, and the
	// request it came from must not wait for that.
	dispatch_async(dispatch_get_main_queue(), ^{
		UIWindow *window = nil;
		for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
			if ([scene isKindOfClass:UIWindowScene.class]) {
				for (UIWindow *candidate in ((UIWindowScene *)scene).windows) {
					if (candidate.isKeyWindow) {
						window = candidate;
						break;
					}
				}
			}
			if (window != nil) {
				break;
			}
		}
		UIViewController *host = window.rootViewController;
		if (host == nil) {
			return;
		}
		// Whatever is already up, or the sheet is presented onto a controller
		// that is covered and never appears.
		while (host.presentedViewController != nil) {
			host = host.presentedViewController;
		}

		UIActivityViewController *sheet =
			[[UIActivityViewController alloc] initWithActivityItems:items
			                                 applicationActivities:nil];
		// An iPad has nowhere to point the sheet at unless it is told; without
		// this it raises rather than appearing.
		if (sheet.popoverPresentationController != nil) {
			sheet.popoverPresentationController.sourceView = host.view;
			sheet.popoverPresentationController.sourceRect =
				CGRectMake(CGRectGetMidX(host.view.bounds), CGRectGetMidY(host.view.bounds), 0, 0);
			sheet.popoverPresentationController.permittedArrowDirections = 0;
		}
		[host presentViewController:sheet animated:YES completion:nil];
	});

	// Said once the sheet has been asked for. Whether the reader goes through
	// with it is between them and the operating system.
	return 0;
}
