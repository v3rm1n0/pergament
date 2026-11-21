import { api, chapterTarget, type Target } from "./api";
import { parseReferences, type Book } from "./bible";

export async function openBibleReference(
  text: string,
  options: {
    bible: { dir: string; books: Book[] } | null;
    toast: (text: string) => void;
    open: (target: Target) => void;
  },
): Promise<boolean> {
  const { bible, toast, open } = options;
  const ref = bible && parseReferences(text, bible.books)?.[0].ref;
  if (!bible || !ref) return false;
  try {
    if (ref.verse != null && (ref.verse < 1 || ref.verse > (await api.lastVerse(bible.dir, ref.book, ref.chapter)))) {
      return false;
    }
  } catch (e) {
    toast(String(e));
    return false;
  }
  open(chapterTarget(bible.dir, ref.book, ref.chapter, ref.verse ?? 1));
  return true;
}
