import Image from "next/image";

import { ForgotPasswordForm } from "./ui/forgot-password-form";

export default function ForgotPasswordPage() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-muted/20 p-6">
      <div className="w-full max-w-md rounded-xl border bg-background p-6 shadow-sm">
        <div className="mb-4 flex items-center justify-center">
          <Image
            src="/logo.png"
            alt="Sync logo"
            width={56}
            height={56}
            priority
            className="rounded-md"
          />
        </div>
        <h1 className="text-xl font-semibold">Forgot password</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Enter your admin email and we will send a reset link if the account
          exists.
        </p>
        <ForgotPasswordForm />
      </div>
    </div>
  );
}
