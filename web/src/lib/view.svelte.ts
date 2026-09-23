/// How a list of works is drawn: covers, or rows.
///
/// A wall of covers is how you recognise something you have seen before; a
/// row is how you read what something is. Neither is right for everybody, so
/// it is a choice, kept between launches like the language is.

const STORED = 'tsuburu.view'

export type Mode = 'covers' | 'rows'

function read(): Mode {
  try {
    return localStorage.getItem(STORED) === 'rows' ? 'rows' : 'covers'
  } catch {
    return 'covers'
  }
}

class View {
  mode = $state<Mode>(read())

  set(mode: Mode) {
    this.mode = mode
    try {
      localStorage.setItem(STORED, mode)
    } catch {
      // A browser with storage switched off still gets to choose, for now.
    }
  }

  get rows() {
    return this.mode === 'rows'
  }
}

export const view = new View()
