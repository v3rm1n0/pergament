import {
  BookOpen,
  CalendarDays,
  Baby,
  Clapperboard,
  Drama,
  Film,
  GraduationCap,
  Globe,
  Headphones,
  HeartHandshake,
  House,
  Leaf,
  Library,
  MessageCircle,
  Mic,
  Music,
  Newspaper,
  Tv,
  Users,
  type LucideIcon,
} from "lucide-react";

/** Words in a category key or name, in the order they are tried, and the icon each one gets. */
const BY_WORD: [RegExp, LucideIcon][] = [
  [/song|music|choir|orchestra/, Music],
  [/drama/, Drama],
  [/child|kid/, Baby],
  [/teen|young|youth/, GraduationCap],
  [/family/, House],
  [/ministry|preach|witness/, MessageCircle],
  [/interview|experience|stories/, Mic],
  [/meeting|workbook/, Users],
  [/event|program|convention|assembly/, CalendarDays],
  [/activit|disaster|relief|builder/, HeartHandshake],
  [/nature|creation|life/, Leaf],
  [/news|magazine|watchtower|awake/, Newspaper],
  [/bible|scripture/, BookOpen],
  [/pub|book|brochure/, Library],
  [/movie|film|cinema/, Clapperboard],
  [/series|show/, Tv],
  [/world|countr|language/, Globe],
];

/** The icon of a Video or Audio category, picked from its key and name; a generic one by kind when nothing fits. */
export function categoryIcon(key: string, name: string, kind: "video" | "audio"): LucideIcon {
  const text = `${key} ${name}`.toLowerCase();
  return BY_WORD.find(([re]) => re.test(text))?.[1] ?? (kind === "audio" ? Headphones : Film);
}
