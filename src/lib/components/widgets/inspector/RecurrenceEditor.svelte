<script>
  let { value = '', onconfirm, oncancel } = $props();

  function parseRrule(s) {
    const def = {
      mode: 'none', weeklyDays: ['MO'], monthlyMode: 'day', monthlyDay: 1,
      ordinalPos: 1, ordinalWd: 'SU', yearlyMonth: 1, yearlyUseOrdinal: false,
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

  const init = parseRrule(value);
  let mode = $state(init.mode);
  let weeklyDays = $state(init.weeklyDays);
  let monthlyMode = $state(init.monthlyMode);
  let monthlyDay = $state(init.monthlyDay);
  let ordinalPos = $state(init.ordinalPos);
  let ordinalWd = $state(init.ordinalWd);
  let yearlyMonth = $state(init.yearlyMonth);
  let yearlyUseOrdinal = $state(init.yearlyUseOrdinal);

  function buildRrule() {
    if (mode === 'none') return '';
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
      <select class="freq-select" bind:value={mode}>
        <option value="none">Nunca</option>
        <option value="hourly">A Cada Hora</option>
        <option value="daily">Diariamente</option>
        <option value="weekdays">Dias de Semana</option>
        <option value="weekends">Fins de Semana</option>
        <option value="weekly">Semanalmente</option>
        <option value="biweekly">Quinzenalmente</option>
        <option value="monthly">Mensalmente</option>
        <option value="quarterly">A Cada 3 Meses</option>
        <option value="semiannual">A Cada 6 Meses</option>
        <option value="yearly">Anualmente</option>
      </select>
    </div>

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
        <label class="radio-row">
          <input type="radio" name="monthly-mode" value="day" bind:group={monthlyMode} />
          <span>Cada</span>
        </label>
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
        <label class="radio-row">
          <input type="radio" name="monthly-mode" value="ordinal" bind:group={monthlyMode} />
          <span>No(a):</span>
        </label>
        {#if monthlyMode === 'ordinal'}
          <div class="ordinal-row">
            <select bind:value={ordinalPos} class="ordinal-select">
              {#each ORDINALS as o}
                <option value={o.value}>{o.label}</option>
              {/each}
            </select>
            <select bind:value={ordinalWd} class="ordinal-select">
              {#each WD_OPTIONS as wd}
                <option value={wd.code}>{wd.label}</option>
              {/each}
            </select>
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
      <label class="checkbox-row">
        <input type="checkbox" bind:checked={yearlyUseOrdinal} />
        <span>No(a):</span>
      </label>
      {#if yearlyUseOrdinal}
        <div class="ordinal-row">
          <select bind:value={ordinalPos} class="ordinal-select">
            {#each ORDINALS as o}
              <option value={o.value}>{o.label}</option>
            {/each}
          </select>
          <select bind:value={ordinalWd} class="ordinal-select">
            {#each WD_OPTIONS as wd}
              <option value={wd.code}>{wd.label}</option>
            {/each}
          </select>
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

  .freq-select, .ordinal-select {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
    border-radius: 6px;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
    padding: 4px 8px;
    cursor: pointer;
  }

  .freq-select { flex: 1; }
  .ordinal-select { flex: 1; }

  .weekday-grid {
    display: flex;
    gap: 4px;
    justify-content: center;
  }

  .wd-btn {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .wd-btn.selected {
    background: var(--color-interactive);
    border-color: var(--color-interactive);
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

  .radio-row input { accent-color: var(--color-interactive); }

  .day-grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 3px;
  }

  .day-btn {
    padding: 4px 2px;
    border-radius: 4px;
    border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 11px;
    cursor: pointer;
    text-align: center;
  }

  .day-btn.selected {
    background: var(--color-interactive);
    border-color: var(--color-interactive);
    color: var(--color-neutral-active);
  }

  .month-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4px;
  }

  .month-btn {
    padding: 6px 4px;
    border-radius: 6px;
    border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 12px;
    cursor: pointer;
    text-align: center;
  }

  .month-btn.selected {
    background: var(--color-interactive);
    border-color: var(--color-interactive);
    color: var(--color-neutral-active);
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

  .checkbox-row input { accent-color: var(--color-interactive); }

  .btn-row {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    margin-top: 4px;
  }

  .btn-cancel {
    padding: 7px 16px;
    border-radius: 7px;
    border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
    background: transparent;
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 13px;
    cursor: pointer;
  }

  .btn-ok {
    padding: 7px 20px;
    border-radius: 7px;
    border: none;
    background: var(--color-interactive);
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
</style>
