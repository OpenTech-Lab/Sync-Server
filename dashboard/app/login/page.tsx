import { redirect } from "next/navigation";

// /admin/login/ is no longer the canonical login route.
// The login lives at /admin/ (app/page.tsx). Redirect old bookmarks.
export default function LoginPage() {
  redirect("/");
}

