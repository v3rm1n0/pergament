import { ChevronRight, Plus, SquarePlus, Tag } from "lucide-react";
import { AppBar } from "@/app";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-10">
      <h2 className="mb-5 flex items-center gap-1 text-[1.35rem] font-semibold text-accent">
        {title} <ChevronRight size={20} />
      </h2>
      <div className="flex flex-col items-center gap-5">{children}</div>
    </section>
  );
}

function Hint({ icon, text }: { icon: React.ReactNode; text: string }) {
  return (
    <div className="flex w-64 items-center gap-4 text-sm text-fg/60">
      {icon}
      <span>{text}</span>
    </div>
  );
}

/** Layout of the personal study area. Notes, tags and playlists need a
 * user data store, which jwlinux does not have yet. */
export function PersonalView() {
  return (
    <>
      <AppBar title="Personal Study" />
      <div className="flex-1 overflow-y-auto px-5 py-8">
        <Section title="Notes and Tags">
          <Hint icon={<Tag size={34} strokeWidth={1.1} />} text="Create tags to organize your notes" />
          <Hint icon={<SquarePlus size={34} strokeWidth={1.1} />} text="Create notes for personal study" />
        </Section>
        <Section title="Playlists">
          <Hint icon={<Plus size={34} strokeWidth={1.1} />} text="Create playlists of images from publications" />
        </Section>
        <p className="text-center text-sm text-muted">Notes, tags and playlists are not available in Pergament yet.</p>
      </div>
    </>
  );
}
