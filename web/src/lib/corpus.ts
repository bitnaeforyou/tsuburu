/// How far the corpus download has got, said in the units a reader waits in.

/// Seconds left, from how long the part already fetched took.
///
/// Nothing until a first shard is in: the listing and the first connection
/// are most of the opening seconds, and a guess made from them is wildly
/// long and then drops.
export function secondsLeft(bytesDone: number, bytesTotal: number, seconds: number): number | null {
  if (bytesDone <= 0 || bytesTotal <= bytesDone || seconds <= 0) return null
  return Math.round((seconds / bytesDone) * (bytesTotal - bytesDone))
}

/// Whole minutes, never zero while there is anything left: "about 0
/// minutes" for the last half minute reads as finished when it is not.
export function minutesLeft(seconds: number): number {
  return Math.max(1, Math.round(seconds / 60))
}

export function megabytes(bytes: number): number {
  return Math.round(bytes / (1024 * 1024))
}
