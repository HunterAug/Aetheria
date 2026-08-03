import { timingSafeEqual } from "node:crypto";
import { NextRequest, NextResponse } from "next/server";
import { PREVIEW_COOKIE_NAME } from "@/lib/config";

// Visiting /<PREVIEW_ACCESS_KEY> sets a cookie that bypasses the IsReleased
// gate for that visitor (see lib/config.ts). Only matches when no static
// route already claims the segment - Next.js resolves static routes like
// /download or /docs before falling back to this dynamic one, so this never
// shadows a real page.
function safeEqual(a: string, b: string): boolean {
  const bufA = Buffer.from(a);
  const bufB = Buffer.from(b);
  if (bufA.length !== bufB.length) return false;
  return timingSafeEqual(bufA, bufB);
}

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ key: string }> },
) {
  const { key } = await params;
  const secret = process.env.PREVIEW_ACCESS_KEY;
  const home = new URL("/", request.url);

  if (!secret || !safeEqual(key, secret)) {
    return NextResponse.redirect(home);
  }

  const response = NextResponse.redirect(home);
  response.cookies.set(PREVIEW_COOKIE_NAME, "1", {
    httpOnly: true,
    secure: true,
    sameSite: "lax",
    maxAge: 60 * 60 * 24 * 30,
    path: "/",
  });
  return response;
}
