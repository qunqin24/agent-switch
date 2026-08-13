import {
  useEffect,
  useState,
  type ChangeEvent,
  type ComponentProps,
} from "react";
import { Input } from "@/components/ui/input";
import { normalizeProviderKey } from "./helpers/providerKeyUtils";

interface ProviderKeyInputProps
  extends Omit<ComponentProps<typeof Input>, "value" | "onChange" | "onBlur"> {
  value: string;
  onValueChange: (value: string) => void;
}

/**
 * Provider keys only accept ASCII, but Chinese IMEs need to keep their pinyin
 * preedit text untouched until composition ends. Normalizing on every keypress
 * causes the browser and React to fight over that preedit range.
 */
export function ProviderKeyInput({
  value,
  onValueChange,
  ...inputProps
}: ProviderKeyInputProps) {
  const [isComposing, setIsComposing] = useState(false);
  const [compositionValue, setCompositionValue] = useState(value);

  useEffect(() => {
    if (!isComposing) {
      setCompositionValue(value);
    }
  }, [isComposing, value]);

  const commitValue = (rawValue: string) => {
    const normalized = normalizeProviderKey(rawValue);
    setCompositionValue(normalized);
    onValueChange(normalized);
  };

  const handleChange = (event: ChangeEvent<HTMLInputElement>) => {
    const nativeEvent = event.nativeEvent as InputEvent;
    if (isComposing || nativeEvent.isComposing) {
      setCompositionValue(event.target.value);
      return;
    }
    commitValue(event.target.value);
  };

  return (
    <Input
      {...inputProps}
      value={isComposing ? compositionValue : value}
      onChange={handleChange}
      onCompositionStart={() => {
        setCompositionValue(value);
        setIsComposing(true);
      }}
      onCompositionEnd={(event) => {
        setIsComposing(false);
        commitValue(event.currentTarget.value);
      }}
      onBlur={(event) => {
        if (isComposing) {
          setIsComposing(false);
          commitValue(event.currentTarget.value);
        }
      }}
    />
  );
}
