<script>
  import { untrack } from 'svelte';
  import * as Select from '$lib/components/ui/select';
  import * as RadioGroup from '$lib/components/ui/radio-group';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { Label } from '$lib/components/ui/label';

  let { value = '', onconfirm, oncancel } = $props();

  function parseRrule(s) {
    const def = {
      mode: 'none', weeklyDays: ['MO'], monthlyMode: 'day', monthlyDay: 1,
      ordinalPos: 1, ordinalWd: 'SU', yearlyMonth: 1, yearlyUseOrdinal: false,
      minuteInterval: 15,
    };
    if (!s) return def;
    const raw = s.replace(/^RRULE:/, '');
    const p = {};
    raw.split(';').forEach(part => {
      const [k, v] = part.split('=');
      if (k) p[k.trim()] = (v || '').trim();
    });
    const freq = p.FREQ || '';
    const interval = parseInt(p.INTERVAL || '1', 10);
    const byday = p.BYDAY || '';
    const bymonthday = parseInt(p.BYMONTHDAY || '0', 10);
    const bymonth = parseInt(p.BYMONTH || '0', 10);

    if (freq === 'MINUTELY') return { ...def, mode: 'minutely', minuteInterval: interval };
    if (freq === 'HOURLY') return { ...def, mode: 'hourly' };
    if (freq === 'DAILY') return { ...def, mode: 'daily' };
    if (freq === 'WEEKLY') {
      if (byday === 'MO,TU,WE,TH,FR') return { ...def, mode: 'weekdays' };
      if (byday === 'SA,SU') return { ...def, mode: 'weekends' };
      const wMode = interval >= 2 ? 'biweekly' : 'weekly';
      return { ...def, mode: wMode, weeklyDays: byday.split(',').filter(Boolean) };
    }
    if (freq === 'MONTHLY') {
      const mMode = interval === 3 ? 'quarterly' : interval === 6 ? 'semiannual' : 'monthly';
      if (byday && /^-?\d/.test(byday)) {
        const m = byday.match(/^(-?\d+)([A-Z]{2})$/);
        const ordinalPos = m ? parseInt(m[1]) : 1;
        const ordinalWd = m ? m[2] : 'SU';
        return { ...def, mode: mMode, monthlyMode: 'ordinal', ordinalPos, ordinalWd };
      }
      return { ...def, mode: mMode, monthlyMode: 'day', monthlyDay: bymonthday || 1 };
    }
    if (freq === 'YEARLY') {
      if (byday && /^-?\d/.test(byday)) {
        const m = byday.match(/^(-?\d+)([A-Z]{2})$/);
        const ordinalPos = m ? parseInt(m[1]) : 1;
        const ordinalWd = m ? m[2] : 'SU';
        return {
          ...def, mode: 'yearly', yearlyMonth: bymonth || 1,
          yearlyUseOrdinal: true, ordinalPos, ordinalWd,
        };
      }
      return { ...def, mode: 'yearly', yearlyMonth: bymonth || 1 };
    }
    return def;
  }

  const init = untrack(() => parseRrule(value));
  let mode = $state(init.mode);
  let minuteInterval = $state(init.minuteInterval);
  let weeklyDays = $state(init.weeklyDays);
  let monthlyMode = $state(init.monthlyMode);
  let monthlyDay = $state(init.monthlyDay);
  let ordinalPos = $state(init.ordinalPos);
  let ordinalWd = $state(init.ordinalWd);
  let yearlyMonth = $state(init.yearlyMonth);
  let yearlyUseOrdinal = $state(init.yearlyUseOrdinal);

  function buildRrule() {
    if (mode === 'none') return '';
    if (mode === 'minutely') {
      const clamped = Math.min(59, Math.max(1, parseInt(String(minuteInterval), 10) || 1));
      return `FREQ=MINUTELY;INTERVAL=${clamped}`;
    }
    if (mode === 'hourly') return 'FREQ=HOURLY;INTERVAL=1';
    if (mode === 'daily') return 'FREQ=DAILY;INTERVAL=1';
    if (mode === 'weekdays') return 'FREQ=WEEKLY;INTERVAL=1;BYDAY=MO,TU,WE,TH,FR';
    if (mode === 'weekends') return 'FREQ=WEEKLY;INTERVAL=1;BYDAY=SA,SU';
    if (mode === 'weekly' || mode === 'biweekly') {
      const interval = mode === 'biweekly' ? 2 : 1;
      const days = weeklyDays.length ? weeklyDays.join(',') : 'MO';
      return `FREQ=WEEKLY;INTERVAL=${interval};BYDAY=${days}`;
    }
    if (mode === 'monthly' || mode === 'quarterly' || mode === 'semiannual') {
      const interval = mode === 'quarterly' ? 3 : mode === 'semiannual' ? 6 : 1;
      if (monthlyMode === 'ordinal') {
        return `FREQ=MONTHLY;INTERVAL=${interval};BYDAY=${ordinalPos}${ordinalWd}`;
      }
      return `FREQ=MONTHLY;INTERVAL=${interval};BYMONTHDAY=${monthlyDay}`;
    }
    if (mode === 'yearly') {
      if (yearlyUseOrdinal) {
        return `FREQ=YEARLY;INTERVAL=1;BYMONTH=${yearlyMonth};BYDAY=${ordinalPos}${ordinalWd}`;
      }
      return `FREQ=YEARLY;INTERVAL=1;BYMONTH=${yearlyMonth}`;
    }
    return '';
  }

  function toggleDay(code) {
    if (weeklyDays.includes(code)) {
      if (weeklyDays.length > 1) weeklyDays = weeklyDays.filter(d => d !== code);
    } else {
      weeklyDays = [...weeklyDays, code];
    }
  }

  function confirm() { onconfirm(buildRrule()); }

  const WEEKDAYS = [
    { code: 'SU', label: 'D' }, { code: 'MO', label: 'S' }, { code: 'TU', label: 'T' },
    { code: 'WE', label: 'Q' }, { code: 'TH', label: 'Q' }, { code: 'FR', label: 'S' },
    { code: 'SA', label: 'S' },
  ];
  const MONTHS = [
    'jan.', 'fev.', 'mar.', 'abr.', 'mai.', 'jun.',
    'jul.', 'ago.', 'set.', 'out.', 'nov.', 'dez.',
  ];
  const ORDINALS = [
    { value: 1, label: 'primeiro(a)' }, { value: 2, label: 'segundo(a)' },
    { value: 3, label: 'terceiro(a)' }, { value: 4, label: 'quarto(a)' },
    { value: -1, label: 'último(a)' },
  ];
  const WD_OPTIONS = [
    { code: 'SU', label: 'Domingo' }, { code: 'MO', label: 'Segunda' },
    { code: 'TU', label: 'Terça' }, { code: 'WE', label: 'Quarta' },
    { code: 'TH', label: 'Quinta' }, { code: 'FR', label: 'Sexta' },
    { code: 'SA', label: 'Sábado' },
  ];
  const DAY_GRID = Array.from({ length: 31 }, (_, i) => i + 1);
</script>

<div class="rrule-editor">
    <!-- Frequência -->
    <div class="field-row">
      <span class="field-label">Frequência:</span>
      <Select.Root
        type="single"
        value={mode}
        onValueChange={(v) => { if (v) mode = v; }}
      >
        <Select.Trigger class="freq-select-trigger">
          <Select.Value />
        </Select.Trigger>
        <Select.Content>
          <Select.Item value="none">Nunca</Select.Item>
          <Select.Item value="minutely">A Cada N Minutos</Select.Item>
          <Select.Item value="hourly">A Cada Hora</Select.Item>
          <Select.Item value="daily">Diariamente</Select.Item>
          <Select.Item value="weekdays">Dias de Semana</Select.Item>
          <Select.Item value="weekends">Fins de Semana</Select.Item>
          <Select.Item value="weekly">Semanalmente</Select.Item>
          <Select.Item value="biweekly">Quinzenalmente</Select.Item>
          <Select.Item value="monthly">Mensalmente</Select.Item>
          <Select.Item value="quarterly">A Cada 3 Meses</Select.Item>
          <Select.Item value="semiannual">A Cada 6 Meses</Select.Item>
          <Select.Item value="yearly">Anualmente</Select.Item>
        </Select.Content>
      </Select.Root>
    </div>

    <!-- Minutos -->
    {#if mode === 'minutely'}
      <div class="field-row">
        <span class="field-label">A cada:</span>
        <input
          class="minute-input"
          type="number"
          min="1"
          max="59"
          step="1"
          value={minuteInterval}
          oninput={(e) => {
            const raw = parseInt(e.currentTarget.value, 10);
            minuteInterval = isNaN(raw) ? 1 : Math.min(59, Math.max(1, raw));
          }}
        />
        <span class="field-label">minuto(s)</span>
      </div>
    {/if}

    <!-- Semanal: seletor de dias -->
    {#if mode === 'weekly' || mode === 'biweekly'}
      <div class="weekday-grid">
        {#each WEEKDAYS as wd}
          <button
            class="wd-btn"
            class:selected={weeklyDays.includes(wd.code)}
            onclick={() => toggleDay(wd.code)}
            type="button"
          >{wd.label}</button>
        {/each}
      </div>
    {/if}

    <!-- Mensal / Trimestral / Semestral -->
    {#if mode === 'monthly' || mode === 'quarterly' || mode === 'semiannual'}
      <div class="monthly-options">
        <RadioGroup.Root
          value={monthlyMode}
          onValueChange={(v) => { if (v) monthlyMode = v; }}
          class="monthly-radio-group"
        >
          <div class="radio-row">
            <RadioGroup.Item value="day" id="monthly-mode-day" />
            <Label for="monthly-mode-day">Cada</Label>
          </div>
          {#if monthlyMode === 'day'}
            <div class="day-grid">
              {#each DAY_GRID as d}
                <button
                  class="day-btn"
                  class:selected={monthlyDay === d}
                  onclick={() => monthlyDay = d}
                  type="button"
                >{d}</button>
              {/each}
            </div>
          {/if}
          <div class="radio-row">
            <RadioGroup.Item value="ordinal" id="monthly-mode-ordinal" />
            <Label for="monthly-mode-ordinal">No(a):</Label>
          </div>
        </RadioGroup.Root>
        {#if monthlyMode === 'ordinal'}
          <div class="ordinal-row">
            <Select.Root
              type="single"
              value={String(ordinalPos)}
              onValueChange={(v) => { if (v) ordinalPos = parseInt(v, 10); }}
            >
              <Select.Trigger class="ordinal-select-trigger">
                <Select.Value />
              </Select.Trigger>
              <Select.Content>
                {#each ORDINALS as o}
                  <Select.Item value={String(o.value)}>{o.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
            <Select.Root
              type="single"
              value={ordinalWd}
              onValueChange={(v) => { if (v) ordinalWd = v; }}
            >
              <Select.Trigger class="ordinal-select-trigger">
                <Select.Value />
              </Select.Trigger>
              <Select.Content>
                {#each WD_OPTIONS as wd}
                  <Select.Item value={wd.code}>{wd.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Anual -->
    {#if mode === 'yearly'}
      <div class="month-grid">
        {#each MONTHS as m, i}
          <button
            class="month-btn"
            class:selected={yearlyMonth === i + 1}
            onclick={() => yearlyMonth = i + 1}
            type="button"
          >{m}</button>
        {/each}
      </div>
      <label class="checkbox-row" for="yearly-ordinal-check">
        <Checkbox
          id="yearly-ordinal-check"
          bind:checked={yearlyUseOrdinal}
        />
        <span>No(a):</span>
      </label>
      {#if yearlyUseOrdinal}
        <div class="ordinal-row">
          <Select.Root
            type="single"
            value={String(ordinalPos)}
            onValueChange={(v) => { if (v) ordinalPos = parseInt(v, 10); }}
          >
            <Select.Trigger class="ordinal-select-trigger">
              <Select.Value />
            </Select.Trigger>
            <Select.Content>
              {#each ORDINALS as o}
                <Select.Item value={String(o.value)}>{o.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          <Select.Root
            type="single"
            value={ordinalWd}
            onValueChange={(v) => { if (v) ordinalWd = v; }}
          >
            <Select.Trigger class="ordinal-select-trigger">
              <Select.Value />
            </Select.Trigger>
            <Select.Content>
              {#each WD_OPTIONS as wd}
                <Select.Item value={wd.code}>{wd.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
      {/if}
    {/if}

    <!-- Botões -->
    <div class="btn-row">
      <button class="btn-cancel" onclick={oncancel} type="button">Cancelar</button>
      <button class="btn-ok" onclick={confirm} type="button">OK</button>
  </div>
</div>

<style>
  .rrule-editor {
    display: flex;
    flex-direction: column;
    gap: 14px;
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--color-neutral-active);
    padding: 10px 0;
  }

  .field-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .field-label {
    flex-shrink: 0;
    color: var(--color-neutral);
    font-size: 13px;
  }

  :global(.freq-select-trigger),
  :global(.ordinal-select-trigger) {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    border: none;
    border-radius: var(--radius);
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
    padding: 4px 8px;
    cursor: pointer;
    height: auto;
  }

  :global(.freq-select-trigger) {
    flex: 1;
  }

  :global(.ordinal-select-trigger) {
    flex: 1;
  }

  :global(.monthly-radio-group) {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .weekday-grid {
    display: flex;
    gap: 4px;
    justify-content: center;
  }

  .wd-btn {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    border: none;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .wd-btn.selected {
    background: var(--color-interactive);
    color: var(--color-neutral-active);
  }

  .monthly-options {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .radio-row {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    color: var(--color-neutral-active);
  }


  .day-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 3px;
  }

  .day-btn {
    padding: 4px 2px;
    border-radius: var(--radius);
    border: none;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 11px;
    cursor: pointer;
    text-align: center;
  }

  .day-btn.selected {
    background: var(--color-interactive);
    color: var(--color-neutral-active);
  }

  .month-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4px;
  }

  .month-btn {
    padding: 6px 4px;
    border-radius: var(--radius);
    border: none;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 12px;
    cursor: pointer;
    text-align: center;
  }

  .month-btn.selected {
    background: var(--color-interactive);
    color: var(--color-neutral-active);
  }

  .minute-input {
    width: 60px;
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    border: none;
    border-radius: var(--radius);
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
    padding: 4px 8px;
    text-align: center;
    appearance: textfield;
    -moz-appearance: textfield;
    outline: none;
  }

  .minute-input::-webkit-inner-spin-button,
  .minute-input::-webkit-outer-spin-button {
    opacity: 1;
  }

  .ordinal-row {
    display: flex;
    gap: 8px;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    color: var(--color-neutral-active);
  }


  .btn-row {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    margin-top: 4px;
  }

  .btn-cancel {
    padding: 7px 16px;
    border-radius: var(--radius);
    border: none;
    background: transparent;
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 13px;
    cursor: pointer;
  }

  .btn-ok {
    padding: 7px 20px;
    border-radius: var(--radius);
    border: none;
    background: var(--color-interactive);
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
</style>
