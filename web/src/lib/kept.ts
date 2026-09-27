/// Reading and writing what this device remembers.
///
/// Storage is not always there to be used. A WebView is handed to an app with
/// `setDomStorageEnabled` false by default, and a browser told to block site
/// data throws on the *accessor* rather than returning nothing - so the first
/// `localStorage.getItem` in a component's setup takes the whole interface
/// down with it and the reader sees a blank screen.
///
/// Every caller here gets a value or a default, and never an exception.

function reading(area: () => Storage, key: string): string | null {
  try {
    return area().getItem(key)
  } catch {
    return null
  }
}

function writing(area: () => Storage, key: string, value: string) {
  try {
    area().setItem(key, value)
  } catch {
    // Somebody reading with storage switched off still gets to read.
  }
}

export function kept(key: string): string | null {
  return reading(() => localStorage, key)
}

export function keep(key: string, value: string) {
  writing(() => localStorage, key, value)
}

/// The same, for what only lasts as long as the tab is open.
export function keptThisVisit(key: string): string | null {
  return reading(() => sessionStorage, key)
}

export function keepThisVisit(key: string, value: string) {
  writing(() => sessionStorage, key, value)
}
