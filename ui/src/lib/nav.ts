import type { Target } from "./api";

export type View =
  | { name: "home" }
  | { name: "publication"; dir: string; tab?: number }
  | { name: "chapters"; dir: string; book: number }
  | { name: "reader"; target: Target; note?: boolean }
  | { name: "online" }
  | { name: "settings" }
  | { name: "library"; tab?: "publications" | "downloaded" }
  | { name: "category"; id: number; title: string }
  | { name: "meetings" }
  | { name: "personal" };

export interface NavState {
  stack: View[];
}

export type NavAction =
  | { type: "push"; view: View }
  | { type: "replace"; view: View }
  | { type: "back" }
  | { type: "root"; view: View };

export const initialNav: NavState = { stack: [{ name: "home" }] };

export function navReducer(state: NavState, action: NavAction): NavState {
  switch (action.type) {
    case "push":
      return { stack: [...state.stack, action.view] };
    case "replace":
      return { stack: [...state.stack.slice(0, -1), action.view] };
    case "back":
      return state.stack.length > 1 ? { stack: state.stack.slice(0, -1) } : state;
    case "root":
      return { stack: [action.view] };
  }
}

export function currentView(state: NavState): View {
  return state.stack[state.stack.length - 1];
}
