import { invoke } from '@tauri-apps/api/core';
import { useForm, SubmitHandler, ValidateResult } from "react-hook-form"
import './App.css'
import { useState } from 'react';
import { ClipLoader } from 'react-spinners';

function App() {
  return (
    <div id="container">
      <ScanParamForm />
    </div>
  )
}

type Inputs = {
  seed: string;
  destination: string;
  ntaddrs: number;
  nzaddrs: number;
  birth_height: number;
  lwd_url: string;
  end_height: number;
}

const ScanParamForm = () => {
  const {
    register,
    handleSubmit,
    setValue,
    formState: { errors },
  } = useForm<Inputs>({
    defaultValues: {
      seed: 'total below tumble rack treat monkey climb service erase rotate ranch fitness warrior sweet scorpion into note minimum wrist because only lottery mule swim',
      destination: 'u1c2v7ugwccldxzawuzgmt05nt258kkh76hc9rzpahyy3admqd4el30en4pka5zmjkxrte37qszwch4w9kyux393unsu6ftrpz0cvxxlm4',
      ntaddrs: 5,
      nzaddrs: 3,
      birth_height: 2757209,
      lwd_url: 'https://zec.rocks',
      end_height: 0,
    }
  })

  const [scanning, setScanning] = useState(false);
  const [height, setHeight] = useState<number|undefined>();

  const onSubmit: SubmitHandler<Inputs> = async (data) => {
    console.log('Form submitted:', data);
    setScanning(true);
    await invoke('init', data);
    var prev_end_height;
    var max_blocks = 1000;
    while (true) {
      const start = Date.now();
      const end_height: number = await invoke('run_scan', 
        {...data, max_blocks: max_blocks});
      const end = Date.now();
      const elapsed = end - start;
      const rescale = 10000 / elapsed;
      const clamped = Math.min(Math.max(rescale, 0.8), 1.2);
      max_blocks = Math.trunc(max_blocks * clamped);
      console.log(max_blocks);

      setValue('end_height', end_height);
      setHeight(end_height);
      if (prev_end_height == end_height) break;
      prev_end_height = end_height;
    }
    await invoke('do_sweep', data);
    setScanning(false);
  };

  return (
    <div className="max-w-md mx-auto mt-10 p-6 bg-white shadow-lg rounded-lg">
      <h2 className="text-2xl font-bold text-gray-800 mb-6 text-center">Scan Parameters</h2>
      {scanning && (
        <div className="overlay">
          <ClipLoader color="#36d7b7" size={80} />
          <div className='text-3xl'>Please be patient, the scan can take hours or even days...</div>
          <div>Height: {height || 0}</div>
        </div>
      )}
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <div>
          <label htmlFor="name" className="label">
            Seed
          </label>
          <input
            type="text"
            id="seed"
            className="input-field"
            {...register("seed", { validate: isValidSeed, required: true })}
          />
          {errors.seed && <div className='text-red-500'>{errors.seed.message}</div>}
        </div>

        <div>
          <label htmlFor="name" className="label">
            Destination Address
          </label>
          <input
            type="text"
            id="destination"
            className="input-field"
            {...register("destination", { validate: isValidAddress, required: true })}
          />
          {errors.destination && <div className='text-red-500'>{errors.destination.message}</div>}
        </div>

        <div>
          <label htmlFor="name" className="label">
            Number of Transparent Addresses
          </label>
          <input
            type="number"
            id="ntaddrs"
            className="input-field"
            {...register("ntaddrs", { required: true, valueAsNumber: true, })}
          />
        </div>

        <div>
          <label htmlFor="name" className="label">
            Number of Shielded (Sapling) Addresses
          </label>
          <input
            type="number"
            id="nzaddrs"
            className="input-field"
            {...register("nzaddrs", { required: true, valueAsNumber: true, })}
          />
        </div>

        <div>
          <label htmlFor="name" className="label">
            Birth Height
          </label>
          <input
            type="number"
            id="birth_height"
            className="input-field"
            {...register("birth_height", { required: true, valueAsNumber: true, })}
          />
        </div>

        <div>
          <label htmlFor="name" className="label">
            LightWallet Server URL
          </label>
          <input
            type="text"
            id="lwd_url"
            className="input-field"
            {...register("lwd_url", { required: true })}
          />
        </div>

        <button
          type="submit"
          className="w-full bg-blue-500 hover:bg-blue-600 text-white font-medium py-2 px-4 rounded-md shadow-sm transition duration-150"
        >
          Submit
        </button>
      </form>
    </div>
  );
};

async function isValidSeed(seed: string): Promise<ValidateResult> {
  const v: boolean = await invoke('is_valid_seed', { seed: seed });
  return v || 'Invalid seed';
}

async function isValidAddress(address: string): Promise<ValidateResult> {
  const v: boolean = await invoke('is_valid_address', { address: address });
  return v || 'Invalid address';
}

export default App
