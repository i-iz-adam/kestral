import CustomInstallerModal from "../../app/CustomInstallerModal";
import type { StepProps } from "../types";

export default function InstallerStep({ onComplete }: StepProps) {
  return (
    <div style={{ position: "relative", minHeight: 400 }}>
      {/* Embedded CustomInstallerModal within the setup wizard flow */}
      <CustomInstallerModal 
        isOpen={true} 
        onClose={onComplete} 
        onComplete={() => {
          setTimeout(onComplete, 1500);
        }} 
      />
    </div>
  );
}
