/// Reading and writing what this device remembers.
///
/// Storage is not always there to be used. A WebView is handed to an app with
/// `setDomStorageEnabled` false by default, and a browser told to block site
/// data throws on the *accessor* rather than returning nothing - so the first
/// `localStorage.getItem` in a component's setup takes the whole interface
/// down with it and the reader sees a blank screen.
///
/// Every caller here gets a value or a default, and never an exception.

function area(session: boolean): Storage | null {
  try {
    return session ? sessionStorage : localStorage
  } catch {
    return null
  }
}

export function kept(key: string): string | null {
  try {
    return area(false)?.getItem(key) ?? null
  } catch {
    return null
  }
}

export function keep(key: string, value: string) {
  try {
    area(false)?.setItem(key, value)
  } catch {
    // Somebody reading with storage switched off still gets to read.
  }
}

export function forKey(key: string) {
  try {
    area(false)?.removeItem(key)
  } catch {
    // As above.
  }
}

/// The same, for what only lasts as long as the tab is open.
export function keptThisVisit(key: string): string | null {
  try {
    return area(true)?.getItem(key) ?? null
  } catch {
    return null
  }
}

export function keepThisVisit(key: string, value: string) {
  try {
    area(true)?.setItem(key, value)
  } catch {
    // As above.
  }
}
