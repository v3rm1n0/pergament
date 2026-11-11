// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useLongPress } from "@/lib/use-long-press";

function Target({ onPress, onClick }: { onPress: (e: { clientX: number; clientY: number }) => void; onClick?: () => void }) {
  return <div data-testid="t" onClick={onClick} {...useLongPress(onPress)} />;
}

const touch = { pointerType: "touch", clientX: 10, clientY: 20, pointerId: 1 };

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useLongPress", () => {
  it("fires once, with the press position, after 500 ms", () => {
    const onPress = vi.fn();
    render(<Target onPress={onPress} />);
    fireEvent.pointerDown(screen.getByTestId("t"), touch);
    vi.advanceTimersByTime(499);
    expect(onPress).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    vi.advanceTimersByTime(2000);
    expect(onPress).toHaveBeenCalledTimes(1);
    expect(onPress).toHaveBeenCalledWith(expect.objectContaining({ clientX: 10, clientY: 20 }));
  });

  it("does not fire when released before 500 ms", () => {
    const onPress = vi.fn();
    render(<Target onPress={onPress} />);
    fireEvent.pointerDown(screen.getByTestId("t"), touch);
    vi.advanceTimersByTime(300);
    fireEvent.pointerUp(screen.getByTestId("t"), touch);
    vi.advanceTimersByTime(1000);
    expect(onPress).not.toHaveBeenCalled();
  });

  it("does not fire once the pointer moved 10 px or more", () => {
    const onPress = vi.fn();
    render(<Target onPress={onPress} />);
    fireEvent.pointerDown(screen.getByTestId("t"), touch);
    fireEvent.pointerMove(screen.getByTestId("t"), { ...touch, clientX: 25 });
    vi.advanceTimersByTime(1000);
    expect(onPress).not.toHaveBeenCalled();
  });

  it("still fires after a movement under 10 px", () => {
    const onPress = vi.fn();
    render(<Target onPress={onPress} />);
    fireEvent.pointerDown(screen.getByTestId("t"), touch);
    fireEvent.pointerMove(screen.getByTestId("t"), { ...touch, clientX: 14 });
    vi.advanceTimersByTime(500);
    expect(onPress).toHaveBeenCalledTimes(1);
  });

  it("does not fire when the page scrolls", () => {
    const onPress = vi.fn();
    render(<Target onPress={onPress} />);
    fireEvent.pointerDown(screen.getByTestId("t"), touch);
    fireEvent.scroll(window);
    vi.advanceTimersByTime(1000);
    expect(onPress).not.toHaveBeenCalled();
  });

  it("leaves a held mouse button alone", () => {
    const onPress = vi.fn();
    render(<Target onPress={onPress} />);
    fireEvent.pointerDown(screen.getByTestId("t"), { ...touch, pointerType: "mouse" });
    vi.advanceTimersByTime(1000);
    expect(onPress).not.toHaveBeenCalled();
  });

  it("swallows the click that follows the release of a fired press", () => {
    const onClick = vi.fn();
    render(<Target onPress={vi.fn()} onClick={onClick} />);
    fireEvent.pointerDown(screen.getByTestId("t"), touch);
    vi.advanceTimersByTime(500);
    fireEvent.pointerUp(screen.getByTestId("t"), touch);
    fireEvent.click(screen.getByTestId("t"));
    expect(onClick).not.toHaveBeenCalled();
  });

  it("lets a later click through when none followed the fired press", () => {
    const onClick = vi.fn();
    render(<Target onPress={vi.fn()} onClick={onClick} />);
    fireEvent.pointerDown(screen.getByTestId("t"), touch);
    vi.advanceTimersByTime(500);
    fireEvent.pointerUp(screen.getByTestId("t"), touch);
    vi.advanceTimersByTime(100);
    fireEvent.click(screen.getByTestId("t"));
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});
