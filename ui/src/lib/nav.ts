import type { Target } from "./api";

export type LibraryTab = "publications" | "video" | "audio" | "updates" | "downloaded";

type ViewKind =
  | { name: "home" }
  | { name: "bible" }
  | { name: "publication"; dir: string; tab?: number }
  | { name: "chapters"; dir: string; book: number }
  | { name: "reader"; target: Target; note?: boolean; noteGuid?: string; fromSearch?: boolean }
  | { name: "online" }
  | { name: "search"; query?: string }
  | { name: "settings" }
  | { name: "library"; tab?: LibraryTab }
  | { name: "category"; id: number; title: string }
  | { name: "media"; key: string; title: string }
  | { name: "meetings"; week?: string }
  | { name: "personal" };

/** A view and how far it was scrolled when its section was left. */
export type View = ViewKind & { scroll?: number };

/** The sections of the icon rail; each keeps its own stack of views. */
export type Section = "home" | "bible" | "library" | "meetings" | "personal" | "settings";

export interface NavState {
  stack: View[];
  /** The rail section the stack belongs to. */
  section: Section;
  /** The stacks of the sections that were left, to continue where the user stopped. */
  saved: Partial<Record<Section, View[]>>;
}

export type NavAction =
  | { type: "push"; view: View }
  | { type: "replace"; view: View }
  | { type: "back" }
  | { type: "root"; view: View }
  /** A rail button: leave the current section for `to`, or start `to` over when it is already open. */
  | {
    type: "switch";
    to: Section;
    view: View;
    scroll?: number;
    /** An explicit non-empty stack for the target section that wins over `view` and over a saved one; an empty one counts as none. */
    stack?: View[];
  };

export const initialNav: NavState = { stack: [{ name: "home" }], section: "home", saved: {} };

export function navReducer(state: NavState, action: NavAction): NavState {
  switch (action.type) {
    case "push":
      return { ...state, stack: [...state.stack, action.view] };
    case "replace":
      return { ...state, stack: [...state.stack.slice(0, -1), action.view] };
    case "back":
      return state.stack.length > 1 ? { ...state, stack: state.stack.slice(0, -1) } : state;
    case "root":
      return { ...state, stack: [action.view] };
    case "switch": {
      const saved = { ...state.saved };
      if (state.section === action.to) {
        delete saved[action.to];
        return { ...state, stack: action.stack?.length ? action.stack : [action.view], saved };
      }
      saved[state.section] = [...state.stack.slice(0, -1), { ...currentView(state), scroll: action.scroll }];
      const stack = action.stack?.length ? action.stack : saved[action.to] ?? [action.view];
      delete saved[action.to];
      return { stack, section: action.to, saved };
    }
  }
}

export function currentView(state: NavState): View {
  return state.stack[state.stack.length - 1];
}
